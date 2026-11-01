use crate::ast::{BinaryOp, Expr, Function, Program, Stmt, UnaryOp};
use crate::lexer::{Token, TokenKind};

/// How deeply statements may nest (blocks and `if`/`else`/`while` bodies; an `else if`
/// ladder counts one level per rung). Every later pass recurses over the AST, so the
/// limits below bound the compiler's own stack use.
pub const MAX_STATEMENT_NESTING: usize = 1000;
/// How deeply expressions may nest: parentheses, unary operators and call arguments.
/// Clang's bracket-depth limit is also 256, and C only requires 63 parenthesized levels.
pub const MAX_EXPR_NESTING: usize = 256;
/// How tall an expression tree may get. `a+b+c+...` is parsed in a loop, so it never
/// nests, but it still builds a left-leaning tree whose height is the number of terms.
pub const MAX_EXPR_HEIGHT: usize = 10_000;

#[derive(Clone, Copy)]
enum Nesting { Statements = 0, Expressions = 1 }
impl Nesting {
    fn limit(self) -> usize { [MAX_STATEMENT_NESTING, MAX_EXPR_NESTING][self as usize] }
    fn noun(self) -> &'static str { ["statements", "expressions"][self as usize] }
}

pub struct Parser { tokens: Vec<Token>, pos: usize, depth: [usize; 2] }
impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0, depth: [0; 2] } }
    /// Run `f` one level deeper in `kind`, failing instead of recursing without bound.
    /// (On an error the parse is abandoned, so the counter need not be restored.)
    fn nested<T>(&mut self, kind: Nesting, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        if self.depth[kind as usize] >= kind.limit() {
            return Err(format!("at character {}: {} nested too deeply (the limit is {} levels)",
                self.tokens[self.pos].offset, kind.noun(), kind.limit()));
        }
        self.depth[kind as usize] += 1;
        let result = f(self);
        self.depth[kind as usize] -= 1;
        result
    }
    fn peek(&self) -> &TokenKind { &self.tokens[self.pos].kind }
    fn peek2(&self) -> &TokenKind { &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)].kind }
    fn take(&mut self) -> Token { let t=self.tokens[self.pos].clone(); self.pos+=1; t }
    fn expect(&mut self, k: TokenKind) -> Result<(), String> {
        let t=self.take(); if t.kind==k { Ok(()) } else { Err(format!("at character {}: expected {k:?}, got {:?}",t.offset,t.kind)) }
    }
    fn ident(&mut self, s: &str) -> Result<(), String> {
        let t=self.take(); if t.kind==TokenKind::Ident(s.into()) { Ok(()) } else { Err(format!("at character {}: expected {s}, got {:?}",t.offset,t.kind)) }
    }
    fn name(&mut self) -> Result<String, String> {
        let t=self.take();
        match t.kind {
            TokenKind::Ident(s) if is_keyword(&s) => Err(format!("at character {}: '{s}' is a keyword and cannot be used as a name",t.offset)),
            TokenKind::Ident(s) => Ok(s),
            other => Err(format!("at character {}: expected identifier, got {other:?}",t.offset)),
        }
    }
    pub fn parse(mut self) -> Result<Program, String> {
        let mut functions = Vec::new();
        while *self.peek() != TokenKind::End { functions.push(self.function()?); }
        Ok(Program { functions })
    }
    /// `int name(int a, int b) { ... }` or a prototype `int name(int a, int b);`.
    fn function(&mut self) -> Result<Function, String> {
        self.ident("int")?; let name=self.name()?; self.expect(TokenKind::LParen)?;
        let mut params=Vec::new();
        if *self.peek()==TokenKind::Ident("void".into()) && *self.peek2()==TokenKind::RParen {
            self.take();
        } else if *self.peek()!=TokenKind::RParen {
            loop {
                self.ident("int")?; params.push(self.name()?);
                if *self.peek()==TokenKind::Comma { self.take(); } else { break; }
            }
        }
        self.expect(TokenKind::RParen)?;
        let body = if *self.peek()==TokenKind::Semicolon { self.take(); None } else { Some(self.block()?) };
        Ok(Function { name, params, body })
    }
    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(TokenKind::LBrace)?; let mut v=Vec::new();
        while *self.peek()!=TokenKind::RBrace {
            if *self.peek()==TokenKind::End { return Err("unterminated block".into()); }
            v.push(self.statement()?);
        }
        self.expect(TokenKind::RBrace)?; Ok(v)
    }
    fn statement(&mut self) -> Result<Stmt, String> { self.nested(Nesting::Statements, Self::statement_unchecked) }
    fn statement_unchecked(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            TokenKind::LBrace => Ok(Stmt::Block(self.block()?)),
            TokenKind::Ident(s) if s=="int" => {
                self.take(); let name=self.name()?;
                let initializer=if *self.peek()==TokenKind::Equal { self.take(); Some(self.expr(0)?) } else { None };
                self.expect(TokenKind::Semicolon)?; Ok(Stmt::Declaration { name, initializer })
            }
            TokenKind::Ident(s) if s=="return" => { self.take(); let e=self.expr(0)?; self.expect(TokenKind::Semicolon)?; Ok(Stmt::Return(e)) }
            TokenKind::Ident(s) if s=="if" => {
                self.take(); self.expect(TokenKind::LParen)?; let condition=self.expr(0)?; self.expect(TokenKind::RParen)?;
                let then_branch=Box::new(self.statement()?);
                let else_branch=if *self.peek()==TokenKind::Ident("else".into()) { self.take(); Some(Box::new(self.statement()?)) } else { None };
                Ok(Stmt::If { condition, then_branch, else_branch })
            }
            TokenKind::Ident(s) if s=="while" => {
                self.take(); self.expect(TokenKind::LParen)?; let condition=self.expr(0)?; self.expect(TokenKind::RParen)?;
                Ok(Stmt::While { condition, body: Box::new(self.statement()?) })
            }
            TokenKind::Ident(_) if *self.peek2()==TokenKind::Equal => {
                let name=self.name()?; self.expect(TokenKind::Equal)?; let value=self.expr(0)?; self.expect(TokenKind::Semicolon)?;
                Ok(Stmt::Assignment { name, value })
            }
            TokenKind::Semicolon => { self.take(); Ok(Stmt::Block(Vec::new())) }
            TokenKind::Ident(_) | TokenKind::Number(_) | TokenKind::LParen | TokenKind::Plus
            | TokenKind::Minus | TokenKind::Tilde | TokenKind::Bang => {
                let e=self.expr(0)?; self.expect(TokenKind::Semicolon)?; Ok(Stmt::Expr(e))
            }
            x => Err(format!("unexpected token at statement start: {x:?}")),
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> { self.expr_with_height(min).map(|(e, _)| e) }
    /// Like `expr`, also returning the height of the tree (a leaf has height 1).
    fn expr_with_height(&mut self, min: u8) -> Result<(Expr, usize), String> {
        let t=self.take();
        let (mut lhs, mut height)=match t.kind {
            TokenKind::Number(n)=>(Expr::Number(n),1),
            TokenKind::Ident(s) if is_keyword(&s) =>
                return Err(format!("at character {}: unexpected keyword '{s}' in expression",t.offset)),
            TokenKind::Ident(s) if *self.peek()==TokenKind::LParen => {
                self.take(); let mut args=Vec::new(); let mut height=1;
                if *self.peek()!=TokenKind::RParen {
                    loop {
                        let (arg,h)=self.nested(Nesting::Expressions, |p| p.expr_with_height(0))?; args.push(arg); height=height.max(h+1);
                        if *self.peek()==TokenKind::Comma { self.take(); } else { break; }
                    }
                }
                self.expect(TokenKind::RParen)?;
                (Expr::Call { name: s, args }, height)
            }
            TokenKind::Ident(s)=>(Expr::Variable(s),1),
            TokenKind::LParen=>{let r=self.nested(Nesting::Expressions, |p| p.expr_with_height(0))?;self.expect(TokenKind::RParen)?;r},
            TokenKind::Plus=>self.unary(UnaryOp::Plus)?,
            TokenKind::Minus=>self.unary(UnaryOp::Minus)?,
            TokenKind::Tilde=>self.unary(UnaryOp::BitNot)?,
            TokenKind::Bang=>self.unary(UnaryOp::LogicalNot)?,
            x=>return Err(format!("at character {}: expected expression, got {x:?}",t.offset)),
        };
        loop {
            let (op,l,r)=match self.peek() {
                TokenKind::PipePipe=>(BinaryOp::LogicalOr,1,2),
                TokenKind::AmpAmp=>(BinaryOp::LogicalAnd,3,4),
                TokenKind::Pipe=>(BinaryOp::BitOr,5,6),
                TokenKind::Caret=>(BinaryOp::BitXor,7,8),
                TokenKind::Amp=>(BinaryOp::BitAnd,9,10),
                TokenKind::EqualEqual=>(BinaryOp::Equal,11,12),
                TokenKind::BangEqual=>(BinaryOp::NotEqual,11,12),
                TokenKind::Less=>(BinaryOp::Less,13,14),
                TokenKind::LessEqual=>(BinaryOp::LessEqual,13,14),
                TokenKind::Greater=>(BinaryOp::Greater,13,14),
                TokenKind::GreaterEqual=>(BinaryOp::GreaterEqual,13,14),
                TokenKind::Plus=>(BinaryOp::Add,15,16),
                TokenKind::Minus=>(BinaryOp::Sub,15,16),
                TokenKind::Star=>(BinaryOp::Mul,17,18),
                TokenKind::Slash=>(BinaryOp::Div,17,18),
                TokenKind::Percent=>(BinaryOp::Rem,17,18),
                _=>break,
            };
            if l<min { break; } let at=self.take().offset; let (rhs,rhs_height)=self.expr_with_height(r)?;
            height=height.max(rhs_height)+1;
            if height > MAX_EXPR_HEIGHT {
                return Err(format!("at character {at}: expression too complex (more than {MAX_EXPR_HEIGHT} levels of operators)"));
            }
            lhs=Expr::Binary { op, left:Box::new(lhs), right:Box::new(rhs) };
        }
        Ok((lhs, height))
    }
    fn unary(&mut self, op: UnaryOp) -> Result<(Expr, usize), String> {
        let (operand, height)=self.nested(Nesting::Expressions, |p| p.expr_with_height(19))?;
        Ok((Expr::Unary { op, expr: Box::new(operand) }, height+1))
    }
}
fn is_keyword(s: &str) -> bool { matches!(s, "int" | "void" | "return" | "if" | "else" | "while") }

#[cfg(test)]
mod tests {
    use super::*; use crate::lexer::lex;
    fn parse(src: &str) -> Result<Program, String> { Parser::new(lex(src).unwrap()).parse() }
    fn main_body(p: &Program) -> &Vec<Stmt> { p.functions[0].body.as_ref().unwrap() }
    #[test] fn parses_statements() { let p=parse("int main(){int x=1;while(x-5)x=x+1;return x;}").unwrap(); assert_eq!(main_body(&p).len(),3); }
    #[test] fn parses_precedence() { let p=parse("int main(){return 1+2<4 && !0;}").unwrap(); assert_eq!(main_body(&p).len(),1); }
    #[test] fn rejects_missing_semicolon() { assert!(parse("int main(){return 1}").is_err()); }
    #[test] fn parses_multiple_functions_and_params() {
        let p=parse("int add(int a, int b){return a+b;} int main(void){return add(1,2);}").unwrap();
        assert_eq!(p.functions.len(),2);
        assert_eq!(p.functions[0].params, vec!["a","b"]);
    }
    #[test] fn parses_prototype_and_void_params() {
        let p=parse("int f(void); int g(); int f(void){return 1;}").unwrap();
        assert!(p.functions[0].body.is_none() && p.functions[1].body.is_none() && p.functions[2].body.is_some());
        assert!(p.functions[0].params.is_empty() && p.functions[1].params.is_empty());
    }
    #[test] fn parses_call_statement_and_nested_calls() {
        let p=parse("int main(){f(g(1),2);return 0;}").unwrap();
        assert!(matches!(&main_body(&p)[0], Stmt::Expr(Expr::Call{args,..}) if args.len()==2));
    }
    #[test] fn parses_empty_statement() { assert!(parse("int main(){;return 0;}").is_ok()); }
    #[test] fn rejects_keyword_names() {
        assert!(parse("int main(){int return=1;return 0;}").is_err());
        assert!(parse("int main(int while){return 0;}").is_err());
        assert!(parse("int main(){else;}").is_err());
    }
    #[test] fn rejects_trailing_comma_in_params() { assert!(parse("int f(int a,){return a;}").is_err()); }
    /// Debug builds have large frames and test threads small stacks: run on a big one.
    fn on_big_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new().stack_size(64 << 20).spawn(f).unwrap().join().unwrap();
    }
    fn in_main(body: &str) -> String { format!("int main(void){{{body}}}") }
    fn too_deep(src: &str, what: &str) {
        let e = parse(src).expect_err("accepted a program nested beyond the limit");
        assert!(e.contains(&format!("{what} nested too deeply")), "{e}");
    }
    #[test] fn parentheses_nesting_is_limited() { on_big_stack(|| {
        let nest = |n: usize| in_main(&format!("return {}1{};", "(".repeat(n), ")".repeat(n)));
        assert!(parse(&nest(MAX_EXPR_NESTING)).is_ok());
        too_deep(&nest(MAX_EXPR_NESTING + 1), "expressions");
        too_deep(&nest(1_000_000), "expressions");
    }); }
    #[test] fn unary_and_call_nesting_is_limited() { on_big_stack(|| {
        let unary = |n: usize| in_main(&format!("return {}1;", "-".repeat(n)));
        assert!(parse(&unary(MAX_EXPR_NESTING)).is_ok());
        too_deep(&unary(MAX_EXPR_NESTING + 1), "expressions");
        let calls = |n: usize| in_main(&format!("return {}1{};", "f(".repeat(n), ")".repeat(n)));
        assert!(parse(&calls(MAX_EXPR_NESTING)).is_ok());
        too_deep(&calls(MAX_EXPR_NESTING + 1), "expressions");
        // Right operands nest as well: 1+(1+(1+( ...
        let right = |n: usize| in_main(&format!("return {}1{};", "1+(".repeat(n), ")".repeat(n)));
        assert!(parse(&right(MAX_EXPR_NESTING)).is_ok());
        too_deep(&right(MAX_EXPR_NESTING + 1), "expressions");
    }); }
    #[test] fn statement_nesting_is_limited() { on_big_stack(|| {
        // `return` is itself a statement one level inside the innermost block.
        let blocks = |n: usize| in_main(&format!("{}return 1;{}", "{".repeat(n), "}".repeat(n)));
        assert!(parse(&blocks(MAX_STATEMENT_NESTING - 1)).is_ok());
        too_deep(&blocks(MAX_STATEMENT_NESTING), "statements");
        let ifs = |n: usize| in_main(&format!("{}return 1;", "if(1)".repeat(n)));
        assert!(parse(&ifs(MAX_STATEMENT_NESTING - 1)).is_ok());
        too_deep(&ifs(MAX_STATEMENT_NESTING), "statements");
        let whiles = |n: usize| in_main(&format!("{};", "while(0)".repeat(n)));
        assert!(parse(&whiles(MAX_STATEMENT_NESTING - 1)).is_ok());
        too_deep(&whiles(MAX_STATEMENT_NESTING), "statements");
        // An `else if` ladder nests one level per rung.
        let ladder = |n: usize| in_main(&format!("{}{{}}", "if(1){}else ".repeat(n)));
        assert!(parse(&ladder(MAX_STATEMENT_NESTING - 1)).is_ok());
        too_deep(&ladder(MAX_STATEMENT_NESTING), "statements");
        too_deep(&blocks(1_000_000), "statements");
    }); }
    #[test] fn long_operator_chains_are_limited_by_tree_height() { on_big_stack(|| {
        let chain = |n: usize| in_main(&format!("return {};", vec!["1"; n].join("+")));
        assert!(parse(&chain(MAX_EXPR_HEIGHT)).is_ok());
        let e = parse(&chain(MAX_EXPR_HEIGHT + 1)).unwrap_err();
        assert!(e.contains("expression too complex"), "{e}");
        assert!(parse(&chain(1_000_000)).is_err());
    }); }
    #[test] fn nesting_depth_is_restored_between_siblings() { on_big_stack(|| {
        // Thousands of sibling sub-expressions, each nested near the limit: only a counter
        // that is decremented on the way out accepts this.
        let group = format!("{}1{}", "(".repeat(MAX_EXPR_NESTING - 1), ")".repeat(MAX_EXPR_NESTING - 1));
        assert!(parse(&in_main(&format!("return {};", vec![group.as_str(); 50].join("*0+")))).is_ok());
        let stmts = "{ { if (1) while (0) ; } }".repeat(3000);
        assert!(parse(&in_main(&stmts)).is_ok());
    }); }
}
