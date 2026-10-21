#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub return_expr: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(i64),
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Plus,
    Minus,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
}
