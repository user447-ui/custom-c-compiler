#[derive(Debug, Clone, PartialEq)]
pub struct Program { pub functions: Vec<Function> }
/// A function definition (`body` is `Some`) or a prototype (`body` is `None`).
/// Every function returns `int`; parameters are `int`.
#[derive(Debug, Clone, PartialEq)]
pub struct Function { pub name: String, pub params: Vec<String>, pub body: Option<Vec<Stmt>> }
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Declaration { name: String, initializer: Option<Expr> },
    Assignment { name: String, value: Expr },
    Return(Expr),
    /// An expression evaluated for its side effects, e.g. `f(1);`.
    Expr(Expr),
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
    Call { name: String, args: Vec<Expr> },
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
