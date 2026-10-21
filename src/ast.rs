#[derive(Debug, Clone, PartialEq)]
pub struct Program { pub statements: Vec<Stmt> }
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Declaration { name: String, initializer: Option<Expr> },
    Assignment { name: String, value: Expr },
    Return(Expr),
    Block(Vec<Stmt>),
    If { condition: Expr, then_branch: Box<Stmt>, else_branch: Option<Box<Stmt>> },
    While { condition: Expr, body: Box<Stmt> },
}
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(i64),
    Variable(String),
    Unary { op: UnaryOp, expr: Box<Expr> },
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp { Plus, Minus, BitNot, LogicalNot }
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Rem,
    BitAnd, BitOr, BitXor,
    Equal, NotEqual, Less, LessEqual, Greater, GreaterEqual,
    LogicalAnd, LogicalOr,
}
