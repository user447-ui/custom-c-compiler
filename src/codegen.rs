use std::collections::HashMap;
use crate::ir::{BlockId, Instruction, IrBinary, IrFunction, IrModule, IrUnary, Terminator, ValueId};
use crate::regalloc::{self, Allocation, Location, ALLOCATABLE_REGISTERS};

/// System V AMD64 integer argument registers: RDI, RSI, RDX, RCX, R8, R9.
const ARG_REGS: [u8; 6] = [7, 6, 2, 1, 8, 9];

/// Machine code for one function plus the `call rel32` sites still to be resolved.
struct FunctionCode { code: Vec<u8>, calls: Vec<(usize, String)> }

/// Emit the whole module: a `_start` stub that calls `main` and exits with its
/// result, followed by every function. Calls are patched once all offsets are known.
pub fn generate(module: &IrModule) -> Result<Vec<u8>, String> {
    let mut code = Vec::new();
    // _start: call main; mov rdi, rax; mov eax, 60 (sys_exit); syscall
    code.extend_from_slice(&[0xe8, 0, 0, 0, 0]);
    code.extend_from_slice(&[0x48, 0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    let mut calls = vec![(1usize, "main".to_string())];
    let mut offsets = HashMap::<String, usize>::new();
    for function in &module.functions {
        if offsets.insert(function.name.clone(), code.len()).is_some() {
            return Err(format!("invalid IR: duplicate function '{}'", function.name));
        }
        let FunctionCode { code: body, calls: sites } = generate_function(function)?;
        let base = code.len();
        code.extend_from_slice(&body);
        calls.extend(sites.into_iter().map(|(at, name)| (base + at, name)));
    }
    for (at, name) in calls {
        let target = *offsets.get(&name).ok_or_else(|| format!("undefined function '{name}'"))?;
        let delta = i32::try_from(target as i64 - (at + 4) as i64).map_err(|_| "call displacement out of range")?;
        code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
    }
    Ok(code)
}

fn generate_function(program: &IrFunction) -> Result<FunctionCode, String> {
    if program.blocks.is_empty() || program.entry >= program.blocks.len() {
        return Err("invalid IR: missing entry block".into());
    }
    // Frame layout, in 8-byte slots addressed as [rbp - 8*k]:
    //   locals | register spills | phi staging | incoming register params | saved callee-saved regs
    let mut local_slots = HashMap::new();
    for (i, name) in program.locals.iter().enumerate() {
        if local_slots.insert(name.clone(), (i as i32 + 1) * 8).is_some() {
            return Err(format!("invalid IR: duplicate local '{name}'"));
        }
    }
    let allocation = regalloc::allocate(program);
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
    let used_registers: Vec<u8> = ALLOCATABLE_REGISTERS.iter().copied()
        .filter(|r| allocation.locations.iter().any(|l| *l == Location::Register(*r))).collect();
    let saved_base = param_base + register_params;
    let saved_regs: Vec<(u8, i32)> = used_registers.iter().enumerate()
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
        code: Vec::new(), local_slots, allocation, spill_base, phi_stage_slots,
        block_offsets: vec![usize::MAX; program.blocks.len() + edge_trampolines.len()],
        patches: Vec::new(), calls: Vec::new(), edge_targets, current_block: program.entry,
        param_slots, saved_regs,
    };
    // Prologue. Incoming argument registers are saved immediately because RCX/RDX
    // are used as scratch registers by the code that follows.
    g.code.extend_from_slice(&[0x55, 0x48, 0x89, 0xe5]);
    if frame != 0 {
        g.code.extend_from_slice(&[0x48, 0x81, 0xec]);
        g.code.extend_from_slice(&frame.to_le_bytes());
    }
    for (i, slot) in g.param_slots.clone().into_iter().enumerate() { g.store_reg(ARG_REGS[i], slot); }
    for (reg, slot) in g.saved_regs.clone() { g.store_reg(reg, slot); }

    for block in &program.blocks {
        if block.id >= program.blocks.len() {
            return Err("invalid IR: block id out of range".into());
        }
        g.current_block = block.id;
        g.block_offsets[block.id] = g.code.len();
        for instruction in &block.instructions {
            if !matches!(instruction, Instruction::Phi { .. }) { g.instruction(instruction)?; }
        }
        let terminator = block.terminator.ok_or_else(|| format!("invalid IR: block {} has no terminator", block.id))?;
        g.terminator(terminator)?;
    }
    for (index, (predecessor, target)) in edge_trampolines.iter().copied().enumerate() {
        let synthetic = program.blocks.len() + index;
        g.block_offsets[synthetic] = g.code.len();
        // Stage all sources first, then write destinations, so phi copies are parallel.
        for instruction in &program.blocks[target].instructions {
            if let Instruction::Phi { dst, incoming } = instruction {
                if let Some((_, src)) = incoming.iter().find(|(pred, _)| *pred == predecessor) {
                    g.load_value_rax(*src)?;
                    let stage = *g.phi_stage_slots.get(dst).ok_or("invalid IR: missing phi staging slot")?;
                    g.store_rax(stage);
                }
            }
        }
        for instruction in &program.blocks[target].instructions {
            if let Instruction::Phi { dst, .. } = instruction {
                let stage = *g.phi_stage_slots.get(dst).ok_or("invalid IR: missing phi staging slot")?;
                g.load_rax(stage);
                g.store_rax_value(*dst)?;
            }
        }
        g.jump(&[0xe9], target);
    }
    for (at, target) in g.patches.clone() {
        let target = *g.block_offsets.get(target).ok_or("invalid IR branch target")?;
        if target == usize::MAX { return Err("invalid IR: unresolved block target".into()); }
        let delta = i32::try_from(target as i64 - (at + 4) as i64)
            .map_err(|_| "branch displacement out of range")?;
        g.code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
    }
    Ok(FunctionCode { code: g.code, calls: g.calls })
}

struct Generator {
    code: Vec<u8>,
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
    saved_regs: Vec<(u8, i32)>,
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
    /// mov dst, <value>, for any 64-bit register `dst` (0..=15).
    fn load_value_into(&mut self, dst: u8, id: ValueId) -> Result<(), String> {
        match self.location(id)? {
            Location::Spill(slot) => self.load_reg(dst, self.spill_displacement(slot)),
            Location::Register(reg) => {
                let rex = 0x48 | if reg >= 8 { 0x04 } else { 0 } | if dst >= 8 { 0x01 } else { 0 };
                self.code.extend_from_slice(&[rex, 0x89, 0xc0 | ((reg & 7) << 3) | (dst & 7)]);
            }
        }
        Ok(())
    }
    fn load_value_rax(&mut self, id: ValueId) -> Result<(), String> { self.load_value_into(0, id) }
    fn load_value_rcx(&mut self, id: ValueId) -> Result<(), String> { self.load_value_into(1, id) }
    fn store_rax_value(&mut self, id: ValueId) -> Result<(), String> {
        match self.location(id)? {
            Location::Spill(slot) => self.store_rax(self.spill_displacement(slot)),
            Location::Register(reg) => {
                let rex = 0x48 | if reg >= 8 { 0x01 } else { 0 };
                self.code.extend_from_slice(&[rex, 0x89, 0xc0 | (reg & 7)]);
            }
        }
        Ok(())
    }
    /// mov [rbp - displacement], reg
    fn store_reg(&mut self, reg: u8, displacement: i32) {
        self.code.extend_from_slice(&[0x48 | if reg >= 8 { 0x04 } else { 0 }, 0x89, 0x85 | ((reg & 7) << 3)]);
        self.code.extend_from_slice(&(-displacement).to_le_bytes());
    }
    /// mov reg, [rbp - displacement]  (a negative displacement addresses above RBP)
    fn load_reg(&mut self, reg: u8, displacement: i32) {
        self.code.extend_from_slice(&[0x48 | if reg >= 8 { 0x04 } else { 0 }, 0x8b, 0x85 | ((reg & 7) << 3)]);
        self.code.extend_from_slice(&(-displacement).to_le_bytes());
    }
    fn load_rax(&mut self, displacement: i32) { self.load_reg(0, displacement) }
    fn store_rax(&mut self, displacement: i32) { self.store_reg(0, displacement) }
    fn jump(&mut self, opcode: &[u8], target: BlockId) {
        self.code.extend_from_slice(opcode);
        let at = self.code.len();
        self.code.extend_from_slice(&[0; 4]);
        self.patches.push((at, target));
    }
    /// Emit `call callee` with System V argument passing: the first six integer
    /// arguments in registers, the rest pushed right-to-left, RSP 16-byte aligned
    /// at the call, caller cleans up the stack, result in RAX.
    fn call(&mut self, dst: ValueId, callee: &str, args: &[ValueId]) -> Result<(), String> {
        let stack_args = args.len().saturating_sub(ARG_REGS.len());
        let padding = stack_args % 2 == 1;
        if padding { self.code.extend_from_slice(&[0x48, 0x83, 0xec, 0x08]); } // sub rsp, 8
        for arg in args.iter().skip(ARG_REGS.len()).rev() {
            self.load_value_rax(*arg)?;
            self.code.push(0x50); // push rax
        }
        for (i, arg) in args.iter().take(ARG_REGS.len()).enumerate() {
            self.load_value_into(ARG_REGS[i], *arg)?;
        }
        self.code.push(0xe8);
        let at = self.code.len();
        self.code.extend_from_slice(&[0; 4]);
        self.calls.push((at, callee.to_string()));
        let cleanup = (stack_args + usize::from(padding)) as u32 * 8;
        if cleanup != 0 {
            self.code.extend_from_slice(&[0x48, 0x81, 0xc4]); // add rsp, imm32
            self.code.extend_from_slice(&cleanup.to_le_bytes());
        }
        self.store_rax_value(dst)
    }
    fn instruction(&mut self, inst: &Instruction) -> Result<(), String> {
        match inst {
            Instruction::Phi { .. } => {
                return Err("invalid IR: phi nodes must be emitted on control-flow edges".into());
            }
            Instruction::Const { dst, value } => {
                self.code.extend_from_slice(&[0x48, 0xb8]);
                self.code.extend_from_slice(&value.to_le_bytes());
                self.store_rax_value(*dst)?;
            }
            Instruction::Param { dst, index } => {
                if let Some(slot) = self.param_slots.get(*index).copied() {
                    self.load_rax(slot);
                } else {
                    // Stack argument #6 is at [rbp + 16], #7 at [rbp + 24], ...
                    self.load_rax(-(16 + 8 * (*index as i32 - ARG_REGS.len() as i32)));
                }
                self.store_rax_value(*dst)?;
            }
            Instruction::Call { dst, callee, args } => self.call(*dst, callee, args)?,
            Instruction::LoadLocal { dst, name } => {
                self.load_rax(self.local(name)?);
                self.store_rax_value(*dst)?;
            }
            Instruction::StoreLocal { name, src } => {
                self.load_value_rax(*src)?;
                self.store_rax(self.local(name)?);
            }
            Instruction::Unary { dst, op, src } => {
                self.load_value_rax(*src)?;
                match op {
                    IrUnary::Plus => {}
                    IrUnary::Negate => self.code.extend_from_slice(&[0x48, 0xf7, 0xd8]),
                    IrUnary::BitNot => self.code.extend_from_slice(&[0x48, 0xf7, 0xd0]),
                    IrUnary::LogicalNot => self.code.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x94, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]),
                }
                self.store_rax_value(*dst)?;
            }
            Instruction::ToBool { dst, src } => {
                self.load_value_rax(*src)?;
                self.code.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x95, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]);
                self.store_rax_value(*dst)?;
            }
            Instruction::Binary { dst, op, lhs, rhs } => {
                self.load_value_rax(*lhs)?;
                self.load_value_rcx(*rhs)?;
                match op {
                    IrBinary::Add => self.code.extend_from_slice(&[0x48, 0x01, 0xc8]),
                    IrBinary::Sub => self.code.extend_from_slice(&[0x48, 0x29, 0xc8]),
                    IrBinary::Mul => self.code.extend_from_slice(&[0x48, 0x0f, 0xaf, 0xc1]),
                    IrBinary::Div | IrBinary::Rem => {
                        self.code.extend_from_slice(&[0x48, 0x99, 0x48, 0xf7, 0xf9]);
                        if *op == IrBinary::Rem { self.code.extend_from_slice(&[0x48, 0x89, 0xd0]); }
                    }
                    IrBinary::BitAnd => self.code.extend_from_slice(&[0x48, 0x21, 0xc8]),
                    IrBinary::BitOr => self.code.extend_from_slice(&[0x48, 0x09, 0xc8]),
                    IrBinary::BitXor => self.code.extend_from_slice(&[0x48, 0x31, 0xc8]),
                    IrBinary::Equal | IrBinary::NotEqual | IrBinary::Less | IrBinary::LessEqual |
                    IrBinary::Greater | IrBinary::GreaterEqual => {
                        self.code.extend_from_slice(&[0x48, 0x39, 0xc8]);
                        let cc = match op {
                            IrBinary::Equal => 0x94, IrBinary::NotEqual => 0x95,
                            IrBinary::Less => 0x9c, IrBinary::LessEqual => 0x9e,
                            IrBinary::Greater => 0x9f, IrBinary::GreaterEqual => 0x9d,
                            _ => unreachable!(),
                        };
                        self.code.extend_from_slice(&[0x0f, cc, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]);
                    }
                }
                self.store_rax_value(*dst)?;
            }
        }
        Ok(())
    }
    fn terminator(&mut self, term: Terminator) -> Result<(), String> {
        match term {
            Terminator::Jump(target) => {
                let target = self.edge_targets.get(&(self.current_block, target)).copied().unwrap_or(target);
                self.jump(&[0xe9], target);
            }
            Terminator::Branch { condition, if_zero, if_nonzero } => {
                self.load_value_rax(condition)?;
                self.code.extend_from_slice(&[0x48, 0x85, 0xc0]);
                let if_zero = self.edge_targets.get(&(self.current_block, if_zero)).copied().unwrap_or(if_zero);
                let if_nonzero = self.edge_targets.get(&(self.current_block, if_nonzero)).copied().unwrap_or(if_nonzero);
                self.jump(&[0x0f, 0x84], if_zero);
                self.jump(&[0xe9], if_nonzero);
            }
            Terminator::Return(value) => {
                self.load_value_rax(value)?;
                // Epilogue: restore callee-saved registers, then `mov rsp, rbp; pop rbp; ret`.
                for (reg, slot) in self.saved_regs.clone() { self.load_reg(reg, slot); }
                self.code.extend_from_slice(&[0x48, 0x89, 0xec, 0x5d, 0xc3]);
            }
        }
        Ok(())
    }
}
