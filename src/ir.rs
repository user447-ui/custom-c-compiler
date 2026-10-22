use crate::ast::{BinaryOp, Expr, Function, Program, Stmt, UnaryOp};

pub type ValueId = usize;
pub type BlockId = usize;

/// A whole translation unit: every function with a body, in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct IrModule { pub functions: Vec<IrFunction> }

#[derive(Debug, Clone, PartialEq)]
pub struct IrFunction {
    pub name: String,
    pub param_count: usize,
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
    /// A 32-bit signed constant. All IR integer values have C `int` semantics:
    /// 32 bits wide, with arithmetic wrapping modulo 2^32.
    Const { dst: ValueId, value: i32 },
    /// The incoming argument number `index` (0-based), read once at function entry.
    Param { dst: ValueId, index: usize },
    /// Call `callee` with `args`; has side effects, so it is never removed as dead code.
    Call { dst: ValueId, callee: String, args: Vec<ValueId> },
    LoadLocal { dst: ValueId, name: String },
    StoreLocal { name: String, src: ValueId },
    Unary { dst: ValueId, op: IrUnary, src: ValueId },
    Binary { dst: ValueId, op: IrBinary, lhs: ValueId, rhs: ValueId },
    ToBool { dst: ValueId, src: ValueId },
    /// Merge values arriving from distinct predecessor blocks (SSA phi node).
    Phi { dst: ValueId, incoming: Vec<(BlockId, ValueId)> },
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

pub fn lower(program: &Program) -> IrModule {
    IrModule {
        functions: program.functions.iter().filter(|f| f.body.is_some()).map(lower_function).collect(),
    }
}

fn lower_function(function: &Function) -> IrFunction {
    let mut builder = Builder { locals: Vec::new(), value_count: 0, blocks: Vec::new(), current: 0 };
    builder.new_block();
    // Parameters become ordinary locals initialised from the incoming arguments.
    for (index, name) in function.params.iter().enumerate() {
        builder.locals.push(name.clone());
        let dst = builder.value();
        builder.emit(Instruction::Param { dst, index });
        builder.emit(Instruction::StoreLocal { name: name.clone(), src: dst });
    }
    for statement in function.body.as_deref().unwrap_or(&[]) { builder.statement(statement); }
    if builder.blocks[builder.current].terminator.is_none() {
        let zero = builder.value();
        builder.emit(Instruction::Const { dst: zero, value: 0 });
        builder.terminate(Terminator::Return(zero));
    }
    IrFunction {
        name: function.name.clone(), param_count: function.params.len(),
        locals: builder.locals, value_count: builder.value_count, blocks: builder.blocks, entry: 0,
    }
}

/// Promote local variables to SSA using dominators, dominance frontiers, and renaming.
/// Existing expression-value phi nodes are preserved.
pub fn promote_locals_to_ssa(module: &mut IrModule) {
    for function in &mut module.functions { promote_function(function); }
}

fn promote_function(program: &mut IrFunction) {
    use std::collections::{HashMap, HashSet};
    let n = program.blocks.len();
    if n == 0 || program.entry >= n { return; }
    fn successors(block: &BasicBlock) -> Vec<BlockId> {
        match block.terminator {
            Some(Terminator::Jump(t)) => vec![t],
            Some(Terminator::Branch { if_zero, if_nonzero, .. }) if if_zero == if_nonzero => vec![if_zero],
            Some(Terminator::Branch { if_zero, if_nonzero, .. }) => vec![if_zero, if_nonzero],
            _ => Vec::new(),
        }
    }
    let mut preds = vec![Vec::<BlockId>::new(); n];
    for block in &program.blocks {
        for succ in successors(block) {
            if succ < n && !preds[succ].contains(&block.id) { preds[succ].push(block.id); }
        }
    }
    let mut reachable = vec![false; n];
    let mut work = vec![program.entry];
    while let Some(b) = work.pop() {
        if b >= n || reachable[b] { continue; }
        reachable[b] = true;
        work.extend(successors(&program.blocks[b]));
    }
    let universe: HashSet<BlockId> = (0..n).filter(|b| reachable[*b]).collect();
    let mut dom = vec![HashSet::<BlockId>::new(); n];
    for b in 0..n {
        if reachable[b] { dom[b] = if b == program.entry { [b].into_iter().collect() } else { universe.clone() }; }
    }
    loop {
        let mut changed = false;
        for b in 0..n {
            if !reachable[b] || b == program.entry { continue; }
            let incoming: Vec<_> = preds[b].iter().copied().filter(|p| reachable[*p]).collect();
            let mut next = if let Some(first) = incoming.first() { dom[*first].clone() } else { HashSet::new() };
            for p in incoming.iter().skip(1) { next.retain(|d| dom[*p].contains(d)); }
            next.insert(b);
            if next != dom[b] { dom[b] = next; changed = true; }
        }
        if !changed { break; }
    }
    let mut idom = vec![None; n];
    for b in 0..n {
        if reachable[b] && b != program.entry {
            idom[b] = dom[b].iter().copied().filter(|d| *d != b).max_by_key(|d| dom[*d].len());
        }
    }
    let mut children = vec![Vec::<BlockId>::new(); n];
    for b in 0..n { if let Some(parent) = idom[b] { children[parent].push(b); } }
    let mut frontier = vec![HashSet::<BlockId>::new(); n];
    for b in 0..n {
        if !reachable[b] || preds[b].iter().filter(|p| reachable[**p]).count() < 2 { continue; }
        for pred in preds[b].iter().copied().filter(|p| reachable[*p]) {
            let mut runner = Some(pred);
            while let Some(r) = runner {
                if Some(r) == idom[b] { break; }
                frontier[r].insert(b);
                runner = idom[r];
            }
        }
    }
    let mut definitions = HashMap::<String, HashSet<BlockId>>::new();
    for block in &program.blocks {
        if reachable[block.id] {
            for inst in &block.instructions {
                if let Instruction::StoreLocal { name, .. } = inst {
                    definitions.entry(name.clone()).or_default().insert(block.id);
                }
            }
        }
    }
    let mut phi_vars = HashMap::<ValueId, String>::new();
    for (name, def_blocks) in definitions {
        let mut has_phi = HashSet::new();
        let mut queue: Vec<_> = def_blocks.iter().copied().collect();
        while let Some(def_block) = queue.pop() {
            for target in frontier[def_block].iter().copied() {
                if !has_phi.insert(target) { continue; }
                let dst = program.value_count;
                program.value_count += 1;
                phi_vars.insert(dst, name.clone());
                program.blocks[target].instructions.insert(0, Instruction::Phi { dst, incoming: Vec::new() });
                if !def_blocks.contains(&target) { queue.push(target); }
            }
        }
    }
    fn resolve(mut value: ValueId, aliases: &HashMap<ValueId, ValueId>) -> ValueId {
        let mut steps = 0;
        while let Some(next) = aliases.get(&value).copied() {
            if next == value || steps > aliases.len() { break; }
            value = next;
            steps += 1;
        }
        value
    }
    fn rename_block(
        b: BlockId, program: &mut IrFunction, children: &[Vec<BlockId>],
        phi_vars: &HashMap<ValueId, String>, stacks: &mut HashMap<String, Vec<ValueId>>,
        aliases: &mut HashMap<ValueId, ValueId>, out_versions: &mut [HashMap<String, ValueId>],
    ) {
        let mut pushed = Vec::<String>::new();
        let original = std::mem::take(&mut program.blocks[b].instructions);
        let mut rewritten = Vec::with_capacity(original.len());
        for inst in original {
            match inst {
                Instruction::Phi { dst, incoming } => {
                    if let Some(name) = phi_vars.get(&dst) {
                        stacks.entry(name.clone()).or_default().push(dst);
                        pushed.push(name.clone());
                        rewritten.push(Instruction::Phi { dst, incoming });
                    } else {
                        rewritten.push(Instruction::Phi { dst, incoming: incoming.into_iter()
                            .map(|(pred, value)| (pred, resolve(value, aliases))).collect() });
                    }
                }
                Instruction::LoadLocal { dst, name } => {
                    if let Some(value) = stacks.get(&name).and_then(|s| s.last()).copied() {
                        aliases.insert(dst, value);
                    } else {
                        rewritten.push(Instruction::LoadLocal { dst, name });
                    }
                }
                Instruction::StoreLocal { name, src } => {
                    stacks.entry(name.clone()).or_default().push(resolve(src, aliases));
                    pushed.push(name);
                }
                Instruction::Const { dst, value } => rewritten.push(Instruction::Const { dst, value }),
                Instruction::Param { dst, index } => rewritten.push(Instruction::Param { dst, index }),
                Instruction::Call { dst, callee, args } => rewritten.push(Instruction::Call {
                    dst, callee, args: args.into_iter().map(|a| resolve(a, aliases)).collect(),
                }),
                Instruction::Unary { dst, op, src } =>
                    rewritten.push(Instruction::Unary { dst, op, src: resolve(src, aliases) }),
                Instruction::Binary { dst, op, lhs, rhs } =>
                    rewritten.push(Instruction::Binary { dst, op, lhs: resolve(lhs, aliases), rhs: resolve(rhs, aliases) }),
                Instruction::ToBool { dst, src } =>
                    rewritten.push(Instruction::ToBool { dst, src: resolve(src, aliases) }),
            }
        }
        program.blocks[b].instructions = rewritten;
        if let Some(term) = program.blocks[b].terminator {
            program.blocks[b].terminator = Some(match term {
                Terminator::Branch { condition, if_zero, if_nonzero } =>
                    Terminator::Branch { condition: resolve(condition, aliases), if_zero, if_nonzero },
                Terminator::Return(value) => Terminator::Return(resolve(value, aliases)),
                other => other,
            });
        }
        for (name, stack) in stacks.iter() {
            if let Some(value) = stack.last() { out_versions[b].insert(name.clone(), *value); }
        }
        for child in children[b].clone() {
            rename_block(child, program, children, phi_vars, stacks, aliases, out_versions);
        }
        for name in pushed.into_iter().rev() {
            if let Some(stack) = stacks.get_mut(&name) { stack.pop(); }
        }
    }
    let mut stacks = HashMap::<String, Vec<ValueId>>::new();
    let mut aliases = HashMap::<ValueId, ValueId>::new();
    let mut out_versions = vec![HashMap::<String, ValueId>::new(); n];
    rename_block(program.entry, program, &children, &phi_vars, &mut stacks, &mut aliases, &mut out_versions);
    for b in 0..n {
        let incoming_preds = preds[b].clone();
        for inst in &mut program.blocks[b].instructions {
            if let Instruction::Phi { dst, incoming } = inst {
                if let Some(name) = phi_vars.get(dst) {
                    *incoming = incoming_preds.iter().filter_map(|pred| {
                        out_versions[*pred].get(name).copied().map(|v| (*pred, v))
                    }).collect();
                } else {
                    for (_, value) in incoming.iter_mut() { *value = resolve(*value, &aliases); }
                }
            }
        }
    }
    for block in &mut program.blocks {
        if let Some(term) = block.terminator {
            block.terminator = Some(match term {
                Terminator::Branch { condition, if_zero, if_nonzero } =>
                    Terminator::Branch { condition: resolve(condition, &aliases), if_zero, if_nonzero },
                Terminator::Return(value) => Terminator::Return(resolve(value, &aliases)),
                other => other,
            });
        }
    }
}

/// Fold constants inside each basic block. Control-flow edges and short-circuit
/// branches are explicit, so this pass never evaluates an untaken branch.
pub fn optimize(mut module: IrModule) -> IrModule {
    module.functions = module.functions.into_iter().map(optimize_function).collect();
    module
}

fn optimize_function(mut program: IrFunction) -> IrFunction {
    for block in &mut program.blocks {
        let mut constants = std::collections::HashMap::<ValueId, i32>::new();
        for inst in &mut block.instructions {
            match inst.clone() {
                Instruction::Const { dst, value } => { constants.insert(dst, value); }
                Instruction::Unary { dst, op, src } => {
                    let folded = constants.get(&src).copied().map(|n| fold_unary(op, n));
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
                        let value = i32::from(n != 0);
                        *inst = Instruction::Const { dst, value }; constants.insert(dst, value);
                    } else { constants.remove(&dst); }
                }
                Instruction::LoadLocal { dst, .. } | Instruction::Phi { dst, .. }
                | Instruction::Param { dst, .. } | Instruction::Call { dst, .. } => { constants.remove(&dst); }
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

fn simplify_constant_branches(program: &mut IrFunction) {
    for block in &mut program.blocks {
        let mut constants = std::collections::HashMap::<ValueId, i32>::new();
        for inst in &block.instructions {
            match inst {
                Instruction::Const { dst, value } => { constants.insert(*dst, *value); }
                Instruction::Unary { dst, op, src } => {
                    let value = constants.get(src).copied().map(|n| fold_unary(*op, n));
                    if let Some(value) = value { constants.insert(*dst, value); }
                }
                Instruction::Binary { dst, op, lhs, rhs } => {
                    if let Some(value) = constants.get(lhs).copied().zip(constants.get(rhs).copied())
                        .and_then(|(a,b)| fold_binary(*op,a,b)) { constants.insert(*dst, value); }
                }
                Instruction::ToBool { dst, src } => {
                    if let Some(n) = constants.get(src).copied() { constants.insert(*dst, i32::from(n != 0)); }
                }
                Instruction::LoadLocal { dst, .. } | Instruction::Phi { dst, .. }
                | Instruction::Param { dst, .. } | Instruction::Call { dst, .. } => { constants.remove(dst); }
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

fn remove_unreachable_blocks(program: &mut IrFunction) {
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
        for inst in &mut block.instructions {
            if let Instruction::Phi { incoming, .. } = inst {
                incoming.retain(|(pred, _)| *pred < remap.len() && remap[*pred] != usize::MAX);
                for (pred, _) in incoming.iter_mut() { *pred = remap[*pred]; }
            }
        }
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

fn eliminate_dead_values(program: &mut IrFunction) {
    loop {
        let mut uses = vec![0usize; program.value_count];
        for block in &program.blocks {
            for inst in &block.instructions {
                match inst {
                    Instruction::LoadLocal { .. } | Instruction::Const { .. } | Instruction::Param { .. } => {}
                    Instruction::Call { args, .. } => {
                        for value in args { if let Some(n) = uses.get_mut(*value) { *n += 1; } }
                    }
                    Instruction::Phi { incoming, .. } => {
                        for (_, value) in incoming { if let Some(n) = uses.get_mut(*value) { *n += 1; } }
                    }
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
                // Stores and calls have side effects and are always kept.
                Instruction::StoreLocal { .. } | Instruction::Call { .. } => true,
                Instruction::Const { dst, .. } | Instruction::LoadLocal { dst, .. } | Instruction::Param { dst, .. } |
                Instruction::Unary { dst, .. } | Instruction::Binary { dst, .. } |
                Instruction::ToBool { dst, .. } | Instruction::Phi { dst, .. } => uses.get(*dst).copied().unwrap_or(0) != 0,
            });
        }
        let after: usize = program.blocks.iter().map(|b| b.instructions.len()).sum();
        if before == after { break; }
    }
}

/// Constant folding follows the semantics of the generated code: 32-bit `int`
/// arithmetic that wraps modulo 2^32 (as with `gcc -fwrapv`).
fn fold_unary(op: IrUnary, n: i32) -> i32 {
    match op {
        IrUnary::Plus => n,
        IrUnary::Negate => n.wrapping_neg(),
        IrUnary::BitNot => !n,
        IrUnary::LogicalNot => i32::from(n == 0),
    }
}

/// Division and remainder are left unfolded when the hardware would fault
/// (division by zero, `INT_MIN / -1`), so the program behaves as it would at run time.
fn fold_binary(op: IrBinary, a: i32, b: i32) -> Option<i32> {
    Some(match op {
        IrBinary::Add => a.wrapping_add(b), IrBinary::Sub => a.wrapping_sub(b),
        IrBinary::Mul => a.wrapping_mul(b), IrBinary::Div => a.checked_div(b)?,
        IrBinary::Rem => a.checked_rem(b)?, IrBinary::BitAnd => a & b,
        IrBinary::BitOr => a | b, IrBinary::BitXor => a ^ b,
        IrBinary::Equal => i32::from(a == b), IrBinary::NotEqual => i32::from(a != b),
        IrBinary::Less => i32::from(a < b), IrBinary::LessEqual => i32::from(a <= b),
        IrBinary::Greater => i32::from(a > b), IrBinary::GreaterEqual => i32::from(a >= b),
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
    fn constant(&mut self, value: i32) -> ValueId {
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
            Stmt::Expr(e) => { self.expression(e); }
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
            Expr::Number(value) => self.constant(i32::try_from(*value).expect("sema guarantees literals fit in int")),
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
            Expr::Call { name, args } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args { values.push(self.expression(arg)); }
                let dst = self.value();
                self.emit(Instruction::Call { dst, callee: name.clone(), args: values });
                dst
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
        let short_value = self.constant(i32::from(!is_and));
        self.terminate(Terminator::Jump(merge_block));
        self.current = rhs_block;
        let rhs = self.expression(right);
        let rhs_value = self.value();
        self.emit(Instruction::ToBool { dst: rhs_value, src: rhs });
        // The RHS may itself contain control flow (nested && / ||), so the block that
        // actually jumps to the merge is the *current* one, not `rhs_block`.
        let rhs_end_block = self.current;
        self.terminate(Terminator::Jump(merge_block));
        self.current = merge_block;
        self.emit(Instruction::Phi {
            dst: result,
            incoming: vec![(short_block, short_value), (rhs_end_block, rhs_value)],
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::Parser, sema};
    /// Lower and optimise a program, returning its first function.
    fn lower_source(source: &str) -> IrFunction {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        optimize(lower(&sema::analyze(&ast).unwrap())).functions.remove(0)
    }
    fn lower_raw(source: &str) -> IrFunction {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        lower(&sema::analyze(&ast).unwrap()).functions.remove(0)
    }
    fn promoted(source: &str) -> IrFunction {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        let mut m = lower(&sema::analyze(&ast).unwrap());
        promote_locals_to_ssa(&mut m);
        m.functions.remove(0)
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
        assert!(!p.blocks.iter().any(|b| matches!(b.terminator, Some(Terminator::Branch { .. }))));
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
    fn short_circuit_result_uses_phi_node() {
        let p = lower_raw("int main(){int x=2;return x && 3;}");
        assert!(p.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Phi { incoming, .. } if incoming.len() == 2)));
    }
    #[test]
    fn folds_constants_inside_blocks() {
        let p = lower_source("int main(){return 2+3*4;}");
        assert!(p.blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Const { value: 14, .. })));
    }

    #[test]
    fn promotes_branch_assigned_local_to_ssa_phi() {
        let p = promoted("int main(){int x=1;if(x<2)x=x+3;else x=x+4;return x;}");
        assert!(!p.blocks.iter().flat_map(|b| &b.instructions).any(|i|
            matches!(i, Instruction::LoadLocal { .. } | Instruction::StoreLocal { .. })));
        assert!(p.blocks.iter().flat_map(|b| &b.instructions).any(|i|
            matches!(i, Instruction::Phi { incoming, .. } if incoming.len() == 2)));
    }

    #[test]
    fn promotes_loop_carried_local_to_ssa_phi() {
        let p = promoted("int main(){int x=0;while(x<5)x=x+1;return x;}");
        assert!(!p.blocks.iter().flat_map(|b| &b.instructions).any(|i|
            matches!(i, Instruction::LoadLocal { .. } | Instruction::StoreLocal { .. })));
        assert!(p.blocks.iter().flat_map(|b| &b.instructions).any(|i|
            matches!(i, Instruction::Phi { incoming, .. } if incoming.len() >= 2)));
    }

    #[test]
    fn lowers_params_and_calls() {
        let ast = Parser::new(lex("int f(int a,int b){return a+b;} int main(){return f(1,2);}").unwrap()).parse().unwrap();
        let m = optimize(lower(&sema::analyze(&ast).unwrap()));
        assert_eq!(m.functions.len(), 2);
        assert_eq!(m.functions[0].param_count, 2);
        let main = &m.functions[1];
        assert!(main.blocks.iter().flat_map(|b| &b.instructions)
            .any(|i| matches!(i, Instruction::Call { callee, args, .. } if callee == "f" && args.len() == 2)));
    }
    #[test]
    fn unused_call_result_is_not_eliminated() {
        let ast = Parser::new(lex("int f(int a){return a;} int main(){f(1);return 0;}").unwrap()).parse().unwrap();
        let m = optimize(lower(&sema::analyze(&ast).unwrap()));
        assert!(m.functions[1].blocks.iter().flat_map(|b| &b.instructions).any(|i| matches!(i, Instruction::Call { .. })));
    }
    #[test]
    fn unused_params_are_dead_but_used_ones_survive() {
        let ast = Parser::new(lex("int f(int a,int b){return b;} int main(){return f(1,2);}").unwrap()).parse().unwrap();
        let mut m = lower(&sema::analyze(&ast).unwrap());
        promote_locals_to_ssa(&mut m);
        let m = optimize(m);
        let params: Vec<usize> = m.functions[0].blocks.iter().flat_map(|b| &b.instructions)
            .filter_map(|i| if let Instruction::Param { index, .. } = i { Some(*index) } else { None }).collect();
        assert_eq!(params, vec![1]);
    }
}
