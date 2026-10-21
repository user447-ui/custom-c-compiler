use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};

pub type ValueId = usize;
pub type BlockId = usize;

#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram {
    pub locals: Vec<String>,
    pub value_count: usize,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    Const { dst: ValueId, value: i64 },
    LoadLocal { dst: ValueId, name: String },
    StoreLocal { name: String, src: ValueId },
    Unary { dst: ValueId, op: IrUnary, src: ValueId },
    Binary { dst: ValueId, op: IrBinary, lhs: ValueId, rhs: ValueId },
    ToBool { dst: ValueId, src: ValueId },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Terminator {
    Jump(BlockId),
    Branch { condition: ValueId, if_zero: BlockId, if_nonzero: BlockId },
    Return(ValueId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IrUnary { Plus, Negate, BitNot, LogicalNot }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IrBinary {
    Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor,
    Equal, NotEqual, Less, LessEqual, Greater, GreaterEqual,
}

pub fn lower(program: &Program) -> IrProgram {
    let mut builder = Builder { locals: Vec::new(), value_count: 0, blocks: Vec::new(), current: 0 };
    builder.new_block();
    for statement in &program.statements { builder.statement(statement); }
    if builder.blocks[builder.current].terminator.is_none() {
        let zero = builder.value();
        builder.emit(Instruction::Const { dst: zero, value: 0 });
        builder.terminate(Terminator::Return(zero));
    }
    IrProgram { locals: builder.locals, value_count: builder.value_count, blocks: builder.blocks, entry: 0 }
}

/// Fold constants inside each basic block. Control-flow edges and short-circuit
/// branches are explicit, so this pass never evaluates an untaken branch.
pub fn optimize(mut program: IrProgram) -> IrProgram {
    for block in &mut program.blocks {
        let mut constants = std::collections::HashMap::<ValueId, i64>::new();
        for inst in &mut block.instructions {
            match inst.clone() {
                Instruction::Const { dst, value } => { constants.insert(dst, value); }
                Instruction::Unary { dst, op, src } => {
                    let folded = constants.get(&src).copied().and_then(|n| match op {
                        IrUnary::Plus => Some(n), IrUnary::Negate => n.checked_neg(),
                        IrUnary::BitNot => Some(!n), IrUnary::LogicalNot => Some(i64::from(n == 0)),
                    });
                    if let Some(value) = folded {
                        *inst = Instruction::Const { dst, value }; constants.insert(dst, value);
                    } else { constants.remove(&dst); }
                }
                Instruction::Binary { dst, op, lhs, rhs } => {
                    let folded = constants.get(&lhs).copied().zip(constants.get(&rhs).copied())
                        .and_then(|(a,b)| fold_binary(op,a,b));
                    if let Some(value) = folded {
                        *inst = Instruction::Const { dst, value }; constants.insert(dst, value);
                    } else { constants.remove(&dst); }
                }
                Instruction::ToBool { dst, src } => {
                    if let Some(n) = constants.get(&src).copied() {
                        let value = i64::from(n != 0);
                        *inst = Instruction::Const { dst, value }; constants.insert(dst, value);
                    } else { constants.remove(&dst); }
                }
                Instruction::LoadLocal { dst, .. } => { constants.remove(&dst); }
                Instruction::StoreLocal { .. } => {}
            }
        }
    }

    // Re-run the inexpensive CFG passes because deleting a branch can expose
    // unreachable blocks and values that are now dead.
    for _ in 0..3 {
        simplify_constant_branches(&mut program);
        remove_unreachable_blocks(&mut program);
        eliminate_dead_values(&mut program);
    }
    program
}

fn simplify_constant_branches(program: &mut IrProgram) {
    for block in &mut program.blocks {
        let mut constants = std::collections::HashMap::<ValueId, i64>::new();
        for inst in &block.instructions {
            match inst {
                Instruction::Const { dst, value } => { constants.insert(*dst, *value); }
                Instruction::Unary { dst, op, src } => {
                    let value = constants.get(src).copied().and_then(|n| match op {
                        IrUnary::Plus => Some(n), IrUnary::Negate => n.checked_neg(),
                        IrUnary::BitNot => Some(!n), IrUnary::LogicalNot => Some(i64::from(n == 0)),
                    });
                    if let Some(value) = value { constants.insert(*dst, value); }
                }
                Instruction::Binary { dst, op, lhs, rhs } => {
                    if let Some(value) = constants.get(lhs).copied().zip(constants.get(rhs).copied())
                        .and_then(|(a,b)| fold_binary(*op,a,b)) { constants.insert(*dst, value); }
                }
                Instruction::ToBool { dst, src } => {
                    if let Some(n) = constants.get(src).copied() { constants.insert(*dst, i64::from(n != 0)); }
                }
                Instruction::LoadLocal { dst, .. } => { constants.remove(dst); }
                Instruction::StoreLocal { .. } => {}
            }
        }
        if let Some(Terminator::Branch { condition, if_zero, if_nonzero }) = block.terminator {
            if let Some(value) = constants.get(&condition).copied() {
                block.terminator = Some(Terminator::Jump(if value == 0 { if_zero } else { if_nonzero }));
            }
        }
    }
}

fn remove_unreachable_blocks(program: &mut IrProgram) {
    if program.blocks.is_empty() || program.entry >= program.blocks.len() { return; }
    let mut reachable = vec![false; program.blocks.len()];
    let mut worklist = vec![program.entry];
    while let Some(id) = worklist.pop() {
        if id >= program.blocks.len() || reachable[id] { continue; }
        reachable[id] = true;
        match program.blocks[id].terminator {
            Some(Terminator::Jump(target)) => worklist.push(target),
            Some(Terminator::Branch { if_zero, if_nonzero, .. }) => {
                worklist.push(if_zero); worklist.push(if_nonzero);
            }
            Some(Terminator::Return(_)) | None => {}
        }
    }
    let mut remap = vec![usize::MAX; program.blocks.len()];
    let mut blocks = Vec::new();
    for (old, block) in program.blocks.iter().enumerate() {
        if reachable[old] {
            remap[old] = blocks.len();
            blocks.push(block.clone());
        }
    }
    for block in &mut blocks {
        block.id = remap[block.id];
        block.terminator = block.terminator.map(|term| match term {
            Terminator::Jump(target) => Terminator::Jump(remap[target]),
            Terminator::Branch { condition, if_zero, if_nonzero } => Terminator::Branch {
                condition, if_zero: remap[if_zero], if_nonzero: remap[if_nonzero],
            },
            other => other,
        });
    }
    program.entry = remap[program.entry];
    program.blocks = blocks;
}

fn eliminate_dead_values(program: &mut IrProgram) {
    loop {
        let mut uses = vec![0usize; program.value_count];
        for block in &program.blocks {
            for inst in &block.instructions {
                match inst {
                    Instruction::LoadLocal { .. } | Instruction::Const { .. } => {}
                    Instruction::StoreLocal { src, .. } | Instruction::ToBool { src, .. } => {
                        if let Some(n) = uses.get_mut(*src) { *n += 1; }
                    }
                    Instruction::Unary { src, .. } => {
                        if let Some(n) = uses.get_mut(*src) { *n += 1; }
                    }
                    Instruction::Binary { lhs, rhs, .. } => {
                        if let Some(n) = uses.get_mut(*lhs) { *n += 1; }
                        if let Some(n) = uses.get_mut(*rhs) { *n += 1; }
                    }
                }
            }
            match block.terminator {
                Some(Terminator::Branch { condition, .. }) | Some(Terminator::Return(condition)) => {
                    if let Some(n) = uses.get_mut(condition) { *n += 1; }
                }
                _ => {}
            }
        }
        let before: usize = program.blocks.iter().map(|b| b.instructions.len()).sum();
        for block in &mut program.blocks {
            block.instructions.retain(|inst| match inst {
                Instruction::StoreLocal { .. } => true,
                Instruction::Const { dst, .. } | Instruction::LoadLocal { dst, .. } |
                Instruction::Unary { dst, .. } | Instruction::Binary { dst, .. } |
                Instruction::ToBool { dst, .. } => uses.get(*dst).copied().unwrap_or(0) != 0,
            });
        }
        let after: usize = program.blocks.iter().map(|b| b.instructions.len()).sum();
        if before == after { break; }
    }
}

fn fold_binary(op: IrBinary, a: i64, b: i64) -> Option<i64> {
    Some(match op {
        IrBinary::Add => a.checked_add(b)?, IrBinary::Sub => a.checked_sub(b)?,
        IrBinary::Mul => a.checked_mul(b)?, IrBinary::Div => a.checked_div(b)?,
        IrBinary::Rem => a.checked_rem(b)?, IrBinary::BitAnd => a & b,
        IrBinary::BitOr => a | b, IrBinary::BitXor => a ^ b,
        IrBinary::Equal => i64::from(a == b), IrBinary::NotEqual => i64::from(a != b),
        IrBinary::Less => i64::from(a < b), IrBinary::LessEqual => i64::from(a <= b),
        IrBinary::Greater => i64::from(a > b), IrBinary::GreaterEqual => i64::from(a >= b),
    })
}

struct Builder {
    locals: Vec<String>,
    value_count: usize,
    blocks: Vec<BasicBlock>,
    current: BlockId,
}
impl Builder {
    fn new_block(&mut self) -> BlockId {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock { id, instructions: Vec::new(), terminator: None });
        id
    }
    fn value(&mut self) -> ValueId { let id = self.value_count; self.value_count += 1; id }
    fn emit(&mut self, instruction: Instruction) { self.blocks[self.current].instructions.push(instruction); }
    fn terminate(&mut self, terminator: Terminator) {
        debug_assert!(self.blocks[self.current].terminator.is_none());
        self.blocks[self.current].terminator = Some(terminator);
    }
    fn open_block(&mut self) {
        if self.blocks[self.current].terminator.is_some() { self.current = self.new_block(); }
    }
    fn jump_if_open(&mut self, target: BlockId) {
        if self.blocks[self.current].terminator.is_none() { self.terminate(Terminator::Jump(target)); }
    }
    fn constant(&mut self, value: i64) -> ValueId {
        let dst = self.value(); self.emit(Instruction::Const { dst, value }); dst
    }
    fn statement(&mut self, s: &Stmt) {
        self.open_block();
        match s {
            Stmt::Declaration { name, initializer } => {
                if !self.locals.contains(name) { self.locals.push(name.clone()); }
                if let Some(e) = initializer {
                    let src = self.expression(e);
                    self.emit(Instruction::StoreLocal { name: name.clone(), src });
                }
            }
            Stmt::Assignment { name, value } => {
                let src = self.expression(value);
                self.emit(Instruction::StoreLocal { name: name.clone(), src });
            }
            Stmt::Return(e) => {
                let value = self.expression(e); self.terminate(Terminator::Return(value));
            }
            Stmt::Block(body) => for statement in body { self.statement(statement); },
            Stmt::If { condition, then_branch, else_branch } => {
                let condition = self.expression(condition);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let merge_block = self.new_block();
                self.terminate(Terminator::Branch { condition, if_zero: else_block, if_nonzero: then_block });
                self.current = then_block; self.statement(then_branch); self.jump_if_open(merge_block);
                self.current = else_block;
                if let Some(other) = else_branch { self.statement(other); }
                self.jump_if_open(merge_block);
                self.current = merge_block;
            }
            Stmt::While { condition, body } => {
                let cond_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();
                self.terminate(Terminator::Jump(cond_block));
                self.current = cond_block;
                let condition = self.expression(condition);
                self.terminate(Terminator::Branch { condition, if_zero: exit_block, if_nonzero: body_block });
                self.current = body_block; self.statement(body); self.jump_if_open(cond_block);
                self.current = exit_block;
            }
        }
    }
    fn expression(&mut self, e: &Expr) -> ValueId {
        match e {
            Expr::Number(value) => self.constant(*value),
            Expr::Variable(name) => {
                let dst = self.value(); self.emit(Instruction::LoadLocal { dst, name: name.clone() }); dst
            }
            Expr::Unary { op, expr } => {
                let src = self.expression(expr); let dst = self.value();
                let op = match op {
                    UnaryOp::Plus => IrUnary::Plus, UnaryOp::Minus => IrUnary::Negate,
                    UnaryOp::BitNot => IrUnary::BitNot, UnaryOp::LogicalNot => IrUnary::LogicalNot,
                };
                self.emit(Instruction::Unary { dst, op, src }); dst
            }
            Expr::Binary { op: BinaryOp::LogicalAnd, left, right } => self.logical(left, right, true),
            Expr::Binary { op: BinaryOp::LogicalOr, left, right } => self.logical(left, right, false),
            Expr::Binary { op, left, right } => {
                let lhs = self.expression(left); let rhs = self.expression(right); let dst = self.value();
                let op = match op {
                    BinaryOp::Add=>IrBinary::Add, BinaryOp::Sub=>IrBinary::Sub, BinaryOp::Mul=>IrBinary::Mul,
                    BinaryOp::Div=>IrBinary::Div, BinaryOp::Rem=>IrBinary::Rem,
                    BinaryOp::BitAnd=>IrBinary::BitAnd, BinaryOp::BitOr=>IrBinary::BitOr, BinaryOp::BitXor=>IrBinary::BitXor,
                    BinaryOp::Equal=>IrBinary::Equal, BinaryOp::NotEqual=>IrBinary::NotEqual,
                    BinaryOp::Less=>IrBinary::Less, BinaryOp::LessEqual=>IrBinary::LessEqual,
                    BinaryOp::Greater=>IrBinary::Greater, BinaryOp::GreaterEqual=>IrBinary::GreaterEqual,
                    BinaryOp::LogicalAnd|BinaryOp::LogicalOr => unreachable!(),
                };
                self.emit(Instruction::Binary { dst, op, lhs, rhs }); dst
            }
        }
    }
    fn logical(&mut self, left: &Expr, right: &Expr, is_and: bool) -> ValueId {
        let lhs = self.expression(left);
        let rhs_block = self.new_block(); let short_block = self.new_block(); let merge_block = self.new_block();
        let result = self.value();
        let (if_zero, if_nonzero) = if is_and { (short_block, rhs_block) } else { (rhs_block, short_block) };
        self.terminate(Terminator::Branch { condition: lhs, if_zero, if_nonzero });
        self.current = short_block;
        self.emit(Instruction::Const { dst: result, value: i64::from(!is_and) });
        self.terminate(Terminator::Jump(merge_block));
        self.current = rhs_block;
        let rhs = self.expression(right);
        self.emit(Instruction::ToBool { dst: result, src: rhs });
        self.terminate(Terminator::Jump(merge_block));
        self.current = merge_block;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::Parser, sema};
    fn lower_source(source: &str) -> IrProgram {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        optimize(lower(&sema::analyze(&ast).unwrap()))
    }
    #[test]
    fn produces_explicit_blocks_and_terminators() {
        let p = lower_source("int main(){int x=0;while(x<3)x=x+1;return x;}");
        assert!(p.blocks.len() >= 4);
        assert!(p.blocks.iter().all(|b| b.terminator.is_some()));
        assert!(p.blocks.iter().any(|b| matches!(b.terminator, Some(Terminator::Branch { .. }))));
    }
    #[test]
    fn logical_operators_use_control_flow() {
        let p = lower_source("int main(){return 0 && (1/0);}");
        assert!(p.blocks.iter().any(|b| matches!(b.terminator, Some(Terminator::Branch { .. }))));
        assert!(!p.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Binary { op: IrBinary::Div, .. })));
        assert!(p.blocks.len() < 4, "constant short-circuit should prune the RHS CFG");
    }
    #[test]
    fn removes_unreachable_blocks_and_dead_values() {
        let p = lower_source("int main(){int x=1;if(0){x=99;}return x;}");
        assert!(!p.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Const { value: 99, .. })));
        assert!(p.blocks.iter().all(|b| b.terminator.is_some()));
    }
    #[test]
    fn folds_constants_inside_blocks() {
        let p = lower_source("int main(){return 2+3*4;}");
        assert!(p.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Const { value: 14, .. })));
    }
}
