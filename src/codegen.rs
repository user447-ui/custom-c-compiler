use std::collections::HashMap;
use crate::ir::{BlockId, Instruction, IrBinary, IrProgram, IrUnary, Terminator, ValueId};

pub fn generate(program: &IrProgram) -> Result<Vec<u8>, String> {
    if program.blocks.is_empty() || program.entry >= program.blocks.len() {
        return Err("invalid IR: missing entry block".into());
    }
    let mut local_slots = HashMap::new();
    let mut next_slot = 1i32;
    for name in &program.locals {
        if local_slots.insert(name.clone(), next_slot * 8).is_some() {
            return Err(format!("duplicate IR local '{name}'"));
        }
        next_slot += 1;
    }
    let mut value_slots = Vec::with_capacity(program.value_count);
    for _ in 0..program.value_count {
        value_slots.push(next_slot * 8);
        next_slot += 1;
    }
    let frame = (((next_slot as usize - 1) * 8 + 15) / 16 * 16) as u32;
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
        code: Vec::new(), local_slots, value_slots,
        block_offsets: vec![usize::MAX; program.blocks.len() + edge_trampolines.len()],
        patches: Vec::new(), edge_targets, current_block: program.entry,
    };
    // Function prologue. Every IR value has a stable stack home, which makes this
    // first direct backend simple and keeps control-flow joins correct without phi nodes.
    g.code.extend_from_slice(&[0x55, 0x48, 0x89, 0xe5]);
    if frame != 0 {
        g.code.extend_from_slice(&[0x48, 0x81, 0xec]);
        g.code.extend_from_slice(&frame.to_le_bytes());
    }
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
        for instruction in &program.blocks[target].instructions {
            if let Instruction::Phi { dst, incoming } = instruction {
                if let Some((_, src)) = incoming.iter().find(|(pred, _)| *pred == predecessor) {
                    let src_slot = g.value(*src)?;
                    let dst_slot = g.value(*dst)?;
                    g.load_rax(src_slot);
                    g.store_rax(dst_slot);
                }
            }
        }
        g.jump(&[0xe9], target);
    }
    }
    for (at, target) in g.patches.clone() {
        let target = *g.block_offsets.get(target).ok_or("invalid IR branch target")?;
        if target == usize::MAX { return Err("invalid IR: unresolved block target".into()); }
        let delta = i32::try_from(target as i64 - (at + 4) as i64)
            .map_err(|_| "branch displacement out of range")?;
        g.code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
    }
    Ok(g.code)
}

struct Generator {
    code: Vec<u8>,
    local_slots: HashMap<String, i32>,
    value_slots: Vec<i32>,
    block_offsets: Vec<usize>,
    patches: Vec<(usize, BlockId)>,
    edge_targets: HashMap<(BlockId, BlockId), BlockId>,
    current_block: BlockId,
}
impl Generator {
    fn local(&self, name: &str) -> Result<i32, String> {
        self.local_slots.get(name).copied()
            .ok_or_else(|| format!("invalid IR: unknown local '{name}'"))
    }
    fn value(&self, id: ValueId) -> Result<i32, String> {
        self.value_slots.get(id).copied()
            .ok_or_else(|| format!("invalid IR: value %{id} out of range"))
    }
    fn load_rax(&mut self, displacement: i32) {
        self.code.extend_from_slice(&[0x48, 0x8b, 0x85]);
        self.code.extend_from_slice(&(-displacement).to_le_bytes());
    }
    fn load_rcx(&mut self, displacement: i32) {
        self.code.extend_from_slice(&[0x48, 0x8b, 0x8d]);
        self.code.extend_from_slice(&(-displacement).to_le_bytes());
    }
    fn store_rax(&mut self, displacement: i32) {
        self.code.extend_from_slice(&[0x48, 0x89, 0x85]);
        self.code.extend_from_slice(&(-displacement).to_le_bytes());
    }
    fn jump(&mut self, opcode: &[u8], target: BlockId) {
        self.code.extend_from_slice(opcode);
        let at = self.code.len();
        self.code.extend_from_slice(&[0; 4]);
        self.patches.push((at, target));
    }
    fn instruction(&mut self, inst: &Instruction) -> Result<(), String> {
        match inst {
            Instruction::Phi { .. } => {
                return Err("invalid IR: phi nodes must be emitted on control-flow edges".into());
            }
            Instruction::Const { dst, value } => {
                self.code.extend_from_slice(&[0x48, 0xb8]);
                self.code.extend_from_slice(&value.to_le_bytes());
                self.store_rax(self.value(*dst)?);
            }
            Instruction::LoadLocal { dst, name } => {
                self.load_rax(self.local(name)?);
                self.store_rax(self.value(*dst)?);
            }
            Instruction::StoreLocal { name, src } => {
                self.load_rax(self.value(*src)?);
                self.store_rax(self.local(name)?);
            }
            Instruction::Unary { dst, op, src } => {
                self.load_rax(self.value(*src)?);
                match op {
                    IrUnary::Plus => {}
                    IrUnary::Negate => self.code.extend_from_slice(&[0x48, 0xf7, 0xd8]),
                    IrUnary::BitNot => self.code.extend_from_slice(&[0x48, 0xf7, 0xd0]),
                    IrUnary::LogicalNot => self.code.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x94, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]),
                }
                self.store_rax(self.value(*dst)?);
            }
            Instruction::ToBool { dst, src } => {
                self.load_rax(self.value(*src)?);
                self.code.extend_from_slice(&[0x48, 0x85, 0xc0, 0x0f, 0x95, 0xc0, 0x48, 0x0f, 0xb6, 0xc0]);
                self.store_rax(self.value(*dst)?);
            }
            Instruction::Binary { dst, op, lhs, rhs } => {
                self.load_rax(self.value(*lhs)?);
                self.load_rcx(self.value(*rhs)?);
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
                self.store_rax(self.value(*dst)?);
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
                self.load_rax(self.value(condition)?);
                self.code.extend_from_slice(&[0x48, 0x85, 0xc0]);
                let if_zero = self.edge_targets.get(&(self.current_block, if_zero)).copied().unwrap_or(if_zero);
                let if_nonzero = self.edge_targets.get(&(self.current_block, if_nonzero)).copied().unwrap_or(if_nonzero);
                self.jump(&[0x0f, 0x84], if_zero);
                self.jump(&[0xe9], if_nonzero);
            }
            Terminator::Return(value) => {
                self.load_rax(self.value(value)?);
                // Linux x86-64 sys_exit(status), intentionally no libc dependency.
                self.code.extend_from_slice(&[0x48, 0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
            }
        }
        Ok(())
    }
}
