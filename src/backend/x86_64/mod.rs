//! x86-64 backend: turns IR functions into machine code that follows the
//! System V AMD64 calling convention.
//!
//! Integer model: C `int` is 32 bits. Every value held in a register or frame
//! slot is the *sign extension* to 64 bits of its 32-bit value ("canonical form").
//! AND/OR/XOR/NOT, comparisons and booleans keep that form automatically;
//! operations that can overflow (add, sub, mul, neg, div) re-canonicalise with
//! `movsxd`. Values entering from outside (parameters, call results) are
//! sign-extended explicitly because the ABI leaves the upper 32 bits undefined.
//!
//! Frame layout, in 8-byte slots addressed as `[rbp - 8*k]`:
//! locals | register spills | phi staging | incoming register params | saved callee-saved regs.

pub mod abi;
pub mod encode;

use std::collections::HashMap;
use crate::backend::{Backend, CallSite, FunctionCode};
use crate::ir::{BlockId, Instruction, IrBinary, IrFunction, IrUnary, Terminator, ValueId};
use crate::regalloc::{self, Allocation, Location, PhysReg};
use crate::target::Os;
use abi::{Reg, ALLOCATABLE_REGISTERS, ARG_REGS, RAX, RCX, RDI};
use encode::{patch_rel32, Asm, Cond};

pub struct X86_64;

impl Backend for X86_64 {
    fn name(&self) -> &'static str { "x86_64" }
    fn allocatable_registers(&self) -> &'static [PhysReg] { &ALLOCATABLE_REGISTERS }
    fn register_name(&self, reg: PhysReg) -> &'static str { abi::register_name(reg) }
    fn compile_function(&self, function: &IrFunction) -> Result<FunctionCode, String> { compile_function(function) }

    /// `_start: call main; mov rdi, rax; mov eax, <exit>; syscall`
    fn entry_stub(&self, os: Os) -> Result<FunctionCode, String> {
        let mut asm = Asm::new();
        let call = asm.call_rel32();
        asm.mov_rr(RDI, RAX);
        asm.mov_eax_imm32(abi::exit_syscall(os));
        asm.syscall();
        Ok(FunctionCode { code: asm.into_bytes(), calls: vec![CallSite { offset: call, callee: "main".into() }] })
    }

    fn patch_call(&self, code: &mut [u8], site: usize, target: usize) -> Result<(), String> {
        patch_rel32(code, site, target, "call")
    }
}

fn compile_function(program: &IrFunction) -> Result<FunctionCode, String> {
    if program.blocks.is_empty() || program.entry >= program.blocks.len() {
        return Err("invalid IR: missing entry block".into());
    }
    let mut local_slots = HashMap::new();
    for (i, name) in program.locals.iter().enumerate() {
        if local_slots.insert(name.clone(), (i as i32 + 1) * 8).is_some() {
            return Err(format!("invalid IR: duplicate local '{name}'"));
        }
    }
    let allocation = regalloc::allocate(program, &ALLOCATABLE_REGISTERS);
    let spill_base = program.locals.len() as i32 * 8;
    let mut phi_stage_slots = HashMap::new();
    for block in &program.blocks {
        for instruction in &block.instructions {
            if let Instruction::Phi { dst, .. } = instruction {
                let stage_index = program.locals.len() + allocation.spill_count + phi_stage_slots.len() + 1;
                phi_stage_slots.insert(*dst, stage_index as i32 * 8);
            }
        }
    }
    let param_base = program.locals.len() + allocation.spill_count + phi_stage_slots.len();
    let register_params = program.param_count.min(ARG_REGS.len());
    let param_slots: Vec<i32> = (0..register_params).map(|i| (param_base + i + 1) as i32 * 8).collect();
    let used_registers: Vec<Reg> = ALLOCATABLE_REGISTERS.iter().copied()
        .filter(|r| allocation.locations.iter().any(|l| *l == Location::Register(*r))).collect();
    let saved_base = param_base + register_params;
    let saved_regs: Vec<(Reg, i32)> = used_registers.iter().enumerate()
        .map(|(j, r)| (*r, (saved_base + j + 1) as i32 * 8)).collect();
    let frame_slots = saved_base + saved_regs.len();
    // Keep RSP 16-byte aligned after the prologue (it is 8 mod 16 on entry, 0 after push rbp).
    let frame = ((frame_slots * 8 + 15) / 16 * 16) as u32;

    // Phi assignments are edge-specific. Redirect only those edges through
    // tiny copy blocks so a conditional branch never executes the other edge's copies.
    let mut edge_trampolines = Vec::<(BlockId, BlockId)>::new();
    for block in &program.blocks {
        let mut targets = Vec::new();
        match block.terminator {
            Some(Terminator::Jump(target)) => targets.push(target),
            Some(Terminator::Branch { if_zero, if_nonzero, .. }) => {
                targets.push(if_zero);
                targets.push(if_nonzero);
            }
            _ => {}
        }
        for target in targets {
            let needs_copy = program.blocks.get(target).is_some_and(|b| b.instructions.iter().any(|i|
                matches!(i, Instruction::Phi { incoming, .. } if incoming.iter().any(|(pred, _)| *pred == block.id))));
            if needs_copy && !edge_trampolines.contains(&(block.id, target)) {
                edge_trampolines.push((block.id, target));
            }
        }
    }
    let edge_targets: HashMap<(BlockId, BlockId), BlockId> = edge_trampolines.iter().enumerate()
        .map(|(i, edge)| (*edge, program.blocks.len() + i)).collect();
    let mut g = Generator {
        asm: Asm::new(), local_slots, allocation, spill_base, phi_stage_slots,
        block_offsets: vec![usize::MAX; program.blocks.len() + edge_trampolines.len()],
        patches: Vec::new(), calls: Vec::new(), edge_targets, current_block: program.entry,
        param_slots, saved_regs,
    };
    // Prologue. Incoming argument registers are saved immediately because RCX/RDX
    // are used as scratch registers by the code that follows.
    g.asm.push_rbp();
    g.asm.mov_rbp_rsp();
    if frame != 0 { g.asm.sub_rsp_imm32(frame); }
    for (i, slot) in g.param_slots.clone().into_iter().enumerate() { g.asm.store_frame(ARG_REGS[i], slot); }
    for (reg, slot) in g.saved_regs.clone() { g.asm.store_frame(reg, slot); }

    for block in &program.blocks {
        if block.id >= program.blocks.len() {
            return Err("invalid IR: block id out of range".into());
        }
        g.current_block = block.id;
        g.block_offsets[block.id] = g.asm.len();
        for instruction in &block.instructions {
            if !matches!(instruction, Instruction::Phi { .. }) { g.instruction(instruction)?; }
        }
        let terminator = block.terminator.ok_or_else(|| format!("invalid IR: block {} has no terminator", block.id))?;
        g.terminator(terminator)?;
    }
    for (index, (predecessor, target)) in edge_trampolines.iter().copied().enumerate() {
        let synthetic = program.blocks.len() + index;
        g.block_offsets[synthetic] = g.asm.len();
        // Stage all sources first, then write destinations, so phi copies are parallel.
        for instruction in &program.blocks[target].instructions {
            if let Instruction::Phi { dst, incoming } = instruction {
                if let Some((_, src)) = incoming.iter().find(|(pred, _)| *pred == predecessor) {
                    g.load_value(RAX, *src)?;
                    let stage = *g.phi_stage_slots.get(dst).ok_or("invalid IR: missing phi staging slot")?;
                    g.asm.store_frame(RAX, stage);
                }
            }
        }
        for instruction in &program.blocks[target].instructions {
            if let Instruction::Phi { dst, .. } = instruction {
                let stage = *g.phi_stage_slots.get(dst).ok_or("invalid IR: missing phi staging slot")?;
                g.asm.load_frame(RAX, stage);
                g.store_rax_value(*dst)?;
            }
        }
        let at = g.asm.jmp_rel32();
        g.patches.push((at, target));
    }
    for (at, target) in g.patches.clone() {
        let target = *g.block_offsets.get(target).ok_or("invalid IR branch target")?;
        if target == usize::MAX { return Err("invalid IR: unresolved block target".into()); }
        patch_rel32(g.asm.bytes_mut(), at, target, "branch")?;
    }
    Ok(FunctionCode {
        code: g.asm.into_bytes(),
        calls: g.calls.into_iter().map(|(offset, callee)| CallSite { offset, callee }).collect(),
    })
}

struct Generator {
    asm: Asm,
    local_slots: HashMap<String, i32>,
    allocation: Allocation,
    spill_base: i32,
    phi_stage_slots: HashMap<ValueId, i32>,
    block_offsets: Vec<usize>,
    patches: Vec<(usize, BlockId)>,
    calls: Vec<(usize, String)>,
    edge_targets: HashMap<(BlockId, BlockId), BlockId>,
    current_block: BlockId,
    /// Frame slots holding the incoming register arguments (indices 0..=5).
    param_slots: Vec<i32>,
    /// Callee-saved registers this function uses, with the slot each is saved in.
    saved_regs: Vec<(Reg, i32)>,
}
impl Generator {
    fn local(&self, name: &str) -> Result<i32, String> {
        self.local_slots.get(name).copied()
            .ok_or_else(|| format!("invalid IR: unknown local '{name}'"))
    }
    fn location(&self, id: ValueId) -> Result<Location, String> {
        self.allocation.locations.get(id).copied()
            .ok_or_else(|| format!("invalid IR: value %{id} out of range"))
    }
    fn spill_displacement(&self, slot: usize) -> i32 { self.spill_base + (slot as i32 + 1) * 8 }
    /// `mov dst, <value>`
    fn load_value(&mut self, dst: Reg, id: ValueId) -> Result<(), String> {
        match self.location(id)? {
            Location::Spill(slot) => self.asm.load_frame(dst, self.spill_displacement(slot)),
            Location::Register(reg) => self.asm.mov_rr(dst, reg),
        }
        Ok(())
    }
    /// `mov <value>, rax`
    fn store_rax_value(&mut self, id: ValueId) -> Result<(), String> {
        match self.location(id)? {
            Location::Spill(slot) => self.asm.store_frame(RAX, self.spill_displacement(slot)),
            Location::Register(reg) => self.asm.mov_rr(reg, RAX),
        }
        Ok(())
    }
    /// Emit `call callee` with System V argument passing: the first six integer
    /// arguments in registers, the rest pushed right-to-left, RSP 16-byte aligned
    /// at the call, caller cleans up the stack, result in RAX.
    fn call(&mut self, dst: ValueId, callee: &str, args: &[ValueId]) -> Result<(), String> {
        let stack_args = args.len().saturating_sub(ARG_REGS.len());
        let padding = stack_args % 2 == 1;
        if padding { self.asm.sub_rsp_imm8(8); }
        for arg in args.iter().skip(ARG_REGS.len()).rev() {
            self.load_value(RAX, *arg)?;
            self.asm.push_rax();
        }
        for (i, arg) in args.iter().take(ARG_REGS.len()).enumerate() {
            self.load_value(ARG_REGS[i], *arg)?;
        }
        let at = self.asm.call_rel32();
        self.calls.push((at, callee.to_string()));
        let cleanup = (stack_args + usize::from(padding)) as u32 * 8;
        if cleanup != 0 { self.asm.add_rsp_imm32(cleanup); }
        self.asm.movsxd_rax_eax(); // the callee returns an `int` in EAX
        self.store_rax_value(dst)
    }
    fn instruction(&mut self, inst: &Instruction) -> Result<(), String> {
        match inst {
            Instruction::Phi { .. } => {
                return Err("invalid IR: phi nodes must be emitted on control-flow edges".into());
            }
            Instruction::Const { dst, value } => {
                self.asm.mov_rax_imm64(i64::from(*value));
                self.store_rax_value(*dst)?;
            }
            Instruction::Param { dst, index } => {
                // Read only the low 32 bits: the caller need not extend the argument.
                if let Some(slot) = self.param_slots.get(*index).copied() {
                    self.asm.load_frame_dword_sx_rax(slot);
                } else {
                    // Stack argument #6 is at [rbp + 16], #7 at [rbp + 24], ...
                    self.asm.load_frame_dword_sx_rax(-(16 + 8 * (*index as i32 - ARG_REGS.len() as i32)));
                }
                self.store_rax_value(*dst)?;
            }
            Instruction::Call { dst, callee, args } => self.call(*dst, callee, args)?,
            Instruction::LoadLocal { dst, name } => {
                self.asm.load_frame(RAX, self.local(name)?);
                self.store_rax_value(*dst)?;
            }
            Instruction::StoreLocal { name, src } => {
                self.load_value(RAX, *src)?;
                self.asm.store_frame(RAX, self.local(name)?);
            }
            Instruction::Unary { dst, op, src } => {
                self.load_value(RAX, *src)?;
                match op {
                    IrUnary::Plus => {}
                    IrUnary::Negate => {
                        self.asm.neg_rax();
                        self.asm.movsxd_rax_eax(); // -INT_MIN wraps
                    }
                    IrUnary::BitNot => self.asm.not_rax(),
                    IrUnary::LogicalNot => { self.asm.test_rax_rax(); self.asm.set_rax_if(Cond::Equal); }
                }
                self.store_rax_value(*dst)?;
            }
            Instruction::ToBool { dst, src } => {
                self.load_value(RAX, *src)?;
                self.asm.test_rax_rax();
                self.asm.set_rax_if(Cond::NotEqual);
                self.store_rax_value(*dst)?;
            }
            Instruction::Binary { dst, op, lhs, rhs } => {
                self.load_value(RAX, *lhs)?;
                self.load_value(RCX, *rhs)?;
                match op {
                    IrBinary::Add => { self.asm.add_rax_rcx(); self.asm.movsxd_rax_eax(); }
                    IrBinary::Sub => { self.asm.sub_rax_rcx(); self.asm.movsxd_rax_eax(); }
                    IrBinary::Mul => { self.asm.imul_rax_rcx(); self.asm.movsxd_rax_eax(); }
                    IrBinary::Div | IrBinary::Rem => {
                        // 32-bit signed division (`cdq; idiv ecx`), which faults on
                        // division by zero and INT_MIN / -1 exactly as C `int` does on x86.
                        self.asm.cdq();
                        self.asm.idiv_ecx();
                        if *op == IrBinary::Div { self.asm.movsxd_rax_eax() } else { self.asm.movsxd_rax_edx() }
                    }
                    IrBinary::BitAnd => self.asm.and_rax_rcx(),
                    IrBinary::BitOr => self.asm.or_rax_rcx(),
                    IrBinary::BitXor => self.asm.xor_rax_rcx(),
                    IrBinary::Equal | IrBinary::NotEqual | IrBinary::Less | IrBinary::LessEqual |
                    IrBinary::Greater | IrBinary::GreaterEqual => {
                        self.asm.cmp_rax_rcx();
                        self.asm.set_rax_if(match op {
                            IrBinary::Equal => Cond::Equal, IrBinary::NotEqual => Cond::NotEqual,
                            IrBinary::Less => Cond::Less, IrBinary::LessEqual => Cond::LessEqual,
                            IrBinary::Greater => Cond::Greater, IrBinary::GreaterEqual => Cond::GreaterEqual,
                            _ => unreachable!(),
                        });
                    }
                }
                self.store_rax_value(*dst)?;
            }
        }
        Ok(())
    }
    /// Where a jump from the current block to `target` must really go (through a
    /// phi-copy trampoline if that edge has phi assignments).
    fn edge(&self, target: BlockId) -> BlockId {
        self.edge_targets.get(&(self.current_block, target)).copied().unwrap_or(target)
    }
    fn terminator(&mut self, term: Terminator) -> Result<(), String> {
        match term {
            Terminator::Jump(target) => {
                let at = self.asm.jmp_rel32();
                self.patches.push((at, self.edge(target)));
            }
            Terminator::Branch { condition, if_zero, if_nonzero } => {
                self.load_value(RAX, condition)?;
                self.asm.test_rax_rax();
                let (if_zero, if_nonzero) = (self.edge(if_zero), self.edge(if_nonzero));
                let at = self.asm.jz_rel32();
                self.patches.push((at, if_zero));
                let at = self.asm.jmp_rel32();
                self.patches.push((at, if_nonzero));
            }
            Terminator::Return(value) => {
                self.load_value(RAX, value)?;
                // Epilogue: restore callee-saved registers, then `mov rsp, rbp; pop rbp; ret`.
                for (reg, slot) in self.saved_regs.clone() { self.asm.load_frame(reg, slot); }
                self.asm.mov_rsp_rbp();
                self.asm.pop_rbp();
                self.asm.ret();
            }
        }
        Ok(())
    }
}
