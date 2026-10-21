use crate::ast::{self, BinaryOp, Expr, Program, Stmt, UnaryOp};

/// Target-independent, typed-by-construction integer IR for the current C subset.
/// Variable names have already been resolved by semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram { pub statements: Vec<IrStmt> }

#[derive(Debug, Clone, PartialEq)]
pub enum IrStmt {
    Declare { name: String, initializer: Option<IrExpr> },
    Store { name: String, value: IrExpr },
    Return(IrExpr),
    Block(Vec<IrStmt>),
    If { condition: IrExpr, then_branch: Box<IrStmt>, else_branch: Option<Box<IrStmt>> },
    While { condition: IrExpr, body: Box<IrStmt> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IrUnary { Plus, Negate, BitNot, LogicalNot }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IrBinary {
    Add, Sub, Mul, Div, Rem, BitAnd, BitOr, BitXor,
    Equal, NotEqual, Less, LessEqual, Greater, GreaterEqual,
    LogicalAnd, LogicalOr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrExpr {
    Const(i64),
    Local(String),
    Unary { op: IrUnary, value: Box<IrExpr> },
    Binary { op: IrBinary, lhs: Box<IrExpr>, rhs: Box<IrExpr> },
}

pub fn lower(program: &Program) -> IrProgram {
    IrProgram { statements: program.statements.iter().map(lower_stmt).collect() }
}

pub fn optimize(program: IrProgram) -> IrProgram {
    IrProgram { statements: program.statements.into_iter().map(opt_stmt).collect() }
}

impl IrProgram {
    /// Temporary bridge for the existing native backend while the backend is migrated
    /// to consume IR instructions directly in a later milestone.
    pub fn to_ast(&self) -> Program {
        Program { statements: self.statements.iter().map(stmt_to_ast).collect() }
    }
}

fn lower_stmt(s: &Stmt) -> IrStmt {
    match s {
        Stmt::Declaration { name, initializer } => IrStmt::Declare {
            name: name.clone(), initializer: initializer.as_ref().map(lower_expr),
        },
        Stmt::Assignment { name, value } => IrStmt::Store { name: name.clone(), value: lower_expr(value) },
        Stmt::Return(e) => IrStmt::Return(lower_expr(e)),
        Stmt::Block(ss) => IrStmt::Block(ss.iter().map(lower_stmt).collect()),
        Stmt::If { condition, then_branch, else_branch } => IrStmt::If {
            condition: lower_expr(condition), then_branch: Box::new(lower_stmt(then_branch)),
            else_branch: else_branch.as_ref().map(|s| Box::new(lower_stmt(s))),
        },
        Stmt::While { condition, body } => IrStmt::While {
            condition: lower_expr(condition), body: Box::new(lower_stmt(body)),
        },
    }
}

fn lower_expr(e: &Expr) -> IrExpr {
    match e {
        Expr::Number(n) => IrExpr::Const(*n),
        Expr::Variable(n) => IrExpr::Local(n.clone()),
        Expr::Unary { op, expr } => IrExpr::Unary {
            op: match op { UnaryOp::Plus=>IrUnary::Plus, UnaryOp::Minus=>IrUnary::Negate,
                UnaryOp::BitNot=>IrUnary::BitNot, UnaryOp::LogicalNot=>IrUnary::LogicalNot },
            value: Box::new(lower_expr(expr)),
        },
        Expr::Binary { op, left, right } => IrExpr::Binary {
            op: match op {
                BinaryOp::Add=>IrBinary::Add, BinaryOp::Sub=>IrBinary::Sub, BinaryOp::Mul=>IrBinary::Mul,
                BinaryOp::Div=>IrBinary::Div, BinaryOp::Rem=>IrBinary::Rem,
                BinaryOp::BitAnd=>IrBinary::BitAnd, BinaryOp::BitOr=>IrBinary::BitOr, BinaryOp::BitXor=>IrBinary::BitXor,
                BinaryOp::Equal=>IrBinary::Equal, BinaryOp::NotEqual=>IrBinary::NotEqual,
                BinaryOp::Less=>IrBinary::Less, BinaryOp::LessEqual=>IrBinary::LessEqual,
                BinaryOp::Greater=>IrBinary::Greater, BinaryOp::GreaterEqual=>IrBinary::GreaterEqual,
                BinaryOp::LogicalAnd=>IrBinary::LogicalAnd, BinaryOp::LogicalOr=>IrBinary::LogicalOr,
            },
            lhs: Box::new(lower_expr(left)), rhs: Box::new(lower_expr(right)),
        },
    }
}

fn opt_stmt(s: IrStmt) -> IrStmt {
    match s {
        IrStmt::Declare { name, initializer } => IrStmt::Declare { name, initializer: initializer.map(opt_expr) },
        IrStmt::Store { name, value } => IrStmt::Store { name, value: opt_expr(value) },
        IrStmt::Return(e) => IrStmt::Return(opt_expr(e)),
        IrStmt::Block(ss) => IrStmt::Block(ss.into_iter().map(opt_stmt).collect()),
        IrStmt::If { condition, then_branch, else_branch } => {
            let condition = opt_expr(condition);
            let then_branch = Box::new(opt_stmt(*then_branch));
            let else_branch = else_branch.map(|s| Box::new(opt_stmt(*s)));
            // Keep control-flow structure intact: only expressions are folded here.
            IrStmt::If { condition, then_branch, else_branch }
        }
        IrStmt::While { condition, body } => IrStmt::While {
            condition: opt_expr(condition), body: Box::new(opt_stmt(*body)),
        },
    }
}

fn opt_expr(e: IrExpr) -> IrExpr {
    match e {
        IrExpr::Const(_) | IrExpr::Local(_) => e,
        IrExpr::Unary { op, value } => {
            let value = opt_expr(*value);
            match value {
                IrExpr::Const(n) => {
                    let folded = match op {
                        IrUnary::Plus => Some(n), IrUnary::Negate => n.checked_neg(),
                        IrUnary::BitNot => Some(!n), IrUnary::LogicalNot => Some(i64::from(n == 0)),
                    };
                    folded.map(IrExpr::Const).unwrap_or(IrExpr::Unary { op, value: Box::new(IrExpr::Const(n)) })
                }
                value => IrExpr::Unary { op, value: Box::new(value) },
            }
        }
        IrExpr::Binary { op, lhs, rhs } => {
            let lhs = opt_expr(*lhs);
            match lhs {
                IrExpr::Const(n) => {
                    if op == IrBinary::LogicalAnd && n == 0 { return IrExpr::Const(0); }
                    if op == IrBinary::LogicalOr && n != 0 { return IrExpr::Const(1); }
                    let rhs = opt_expr(*rhs);
                    match rhs {
                        IrExpr::Const(r) => fold_binary(op, n, r).map(IrExpr::Const).unwrap_or(
                            IrExpr::Binary { op, lhs: Box::new(IrExpr::Const(n)), rhs: Box::new(IrExpr::Const(r)) }),
                        rhs => IrExpr::Binary { op, lhs: Box::new(IrExpr::Const(n)), rhs: Box::new(rhs) },
                    }
                }
                lhs => {
                    let rhs = opt_expr(*rhs);
                    IrExpr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) }
                }
            }
        }
    }
}

fn fold_binary(op: IrBinary, a: i64, b: i64) -> Option<i64> {
    Some(match op {
        IrBinary::Add => a.checked_add(b)?,
        IrBinary::Sub => a.checked_sub(b)?,
        IrBinary::Mul => a.checked_mul(b)?,
        IrBinary::Div => a.checked_div(b)?,
        IrBinary::Rem => a.checked_rem(b)?,
        IrBinary::BitAnd => a & b, IrBinary::BitOr => a | b, IrBinary::BitXor => a ^ b,
        IrBinary::Equal => i64::from(a == b), IrBinary::NotEqual => i64::from(a != b),
        IrBinary::Less => i64::from(a < b), IrBinary::LessEqual => i64::from(a <= b),
        IrBinary::Greater => i64::from(a > b), IrBinary::GreaterEqual => i64::from(a >= b),
        IrBinary::LogicalAnd => i64::from(a != 0 && b != 0),
        IrBinary::LogicalOr => i64::from(a != 0 || b != 0),
    })
}

fn stmt_to_ast(s: &IrStmt) -> Stmt {
    match s {
        IrStmt::Declare { name, initializer } => Stmt::Declaration { name: name.clone(), initializer: initializer.as_ref().map(expr_to_ast) },
        IrStmt::Store { name, value } => Stmt::Assignment { name: name.clone(), value: expr_to_ast(value) },
        IrStmt::Return(e) => Stmt::Return(expr_to_ast(e)),
        IrStmt::Block(ss) => Stmt::Block(ss.iter().map(stmt_to_ast).collect()),
        IrStmt::If { condition, then_branch, else_branch } => Stmt::If {
            condition: expr_to_ast(condition), then_branch: Box::new(stmt_to_ast(then_branch)),
            else_branch: else_branch.as_ref().map(|s| Box::new(stmt_to_ast(s))),
        },
        IrStmt::While { condition, body } => Stmt::While { condition: expr_to_ast(condition), body: Box::new(stmt_to_ast(body)) },
    }
}
fn expr_to_ast(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Const(n) => Expr::Number(*n), IrExpr::Local(n) => Expr::Variable(n.clone()),
        IrExpr::Unary { op, value } => Expr::Unary { op: match op {
            IrUnary::Plus=>UnaryOp::Plus, IrUnary::Negate=>UnaryOp::Minus,
            IrUnary::BitNot=>UnaryOp::BitNot, IrUnary::LogicalNot=>UnaryOp::LogicalNot,
        }, expr: Box::new(expr_to_ast(value)) },
        IrExpr::Binary { op, lhs, rhs } => Expr::Binary { op: match op {
            IrBinary::Add=>BinaryOp::Add, IrBinary::Sub=>BinaryOp::Sub, IrBinary::Mul=>BinaryOp::Mul,
            IrBinary::Div=>BinaryOp::Div, IrBinary::Rem=>BinaryOp::Rem,
            IrBinary::BitAnd=>BinaryOp::BitAnd, IrBinary::BitOr=>BinaryOp::BitOr, IrBinary::BitXor=>BinaryOp::BitXor,
            IrBinary::Equal=>BinaryOp::Equal, IrBinary::NotEqual=>BinaryOp::NotEqual,
            IrBinary::Less=>BinaryOp::Less, IrBinary::LessEqual=>BinaryOp::LessEqual,
            IrBinary::Greater=>BinaryOp::Greater, IrBinary::GreaterEqual=>BinaryOp::GreaterEqual,
            IrBinary::LogicalAnd=>BinaryOp::LogicalAnd, IrBinary::LogicalOr=>BinaryOp::LogicalOr,
        }, left: Box::new(expr_to_ast(lhs)), right: Box::new(expr_to_ast(rhs)) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::Parser, sema};
    fn lower_source(s: &str) -> IrProgram {
        let ast = Parser::new(lex(s).unwrap()).parse().unwrap();
        optimize(lower(&sema::analyze(&ast).unwrap()))
    }
    #[test] fn folds_arithmetic_and_comparisons() {
        let p=lower_source("int main(){return 2+3*4==14;}");
        assert_eq!(p.statements, vec![IrStmt::Return(IrExpr::Const(1))]);
    }
    #[test] fn folds_unary_ops() {
        let p=lower_source("int main(){return !0 + ~0;}");
        assert_eq!(p.statements, vec![IrStmt::Return(IrExpr::Const(-1))]);
    }
    #[test] fn folds_short_circuit_without_evaluating_unreachable_rhs() {
        let p=lower_source("int main(){return 0 && (1/0);}");
        assert_eq!(p.statements, vec![IrStmt::Return(IrExpr::Const(0))]);
        let p=lower_source("int main(){return 1 || (1/0);}");
        assert_eq!(p.statements, vec![IrStmt::Return(IrExpr::Const(1))]);
    }
    #[test] fn does_not_fold_division_by_zero() {
        let p=lower_source("int main(){return 1/0;}");
        assert!(matches!(&p.statements[0], IrStmt::Return(IrExpr::Binary { op: IrBinary::Div, .. })));
    }
    #[test] fn leaves_runtime_expressions_alone() {
        let p=lower_source("int main(){int x=2;return x+3*4;}");
        let IrStmt::Return(IrExpr::Binary { lhs, rhs, .. })=&p.statements[1] else { panic!() };
        assert!(matches!(lhs.as_ref(),IrExpr::Local(_)));
        assert_eq!(rhs.as_ref(),&IrExpr::Const(12));
    }
}
