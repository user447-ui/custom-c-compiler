use crate::ast::{BinaryOp, Expr, Function, Program, Stmt, UnaryOp};
use crate::lexer::{Token, TokenKind};

pub struct Parser { tokens: Vec<Token>, pos: usize }
impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self { Self { tokens, pos: 0 } }
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
    fn statement(&mut self) -> Result<Stmt, String> {
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
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let t=self.take();
        let mut lhs=match t.kind {
            TokenKind::Number(n)=>Expr::Number(n),
            TokenKind::Ident(s) if is_keyword(&s) =>
                return Err(format!("at character {}: unexpected keyword '{s}' in expression",t.offset)),
            TokenKind::Ident(s) if *self.peek()==TokenKind::LParen => {
                self.take(); let mut args=Vec::new();
                if *self.peek()!=TokenKind::RParen {
                    loop {
                        args.push(self.expr(0)?);
                        if *self.peek()==TokenKind::Comma { self.take(); } else { break; }
                    }
                }
                self.expect(TokenKind::RParen)?;
                Expr::Call { name: s, args }
            }
            TokenKind::Ident(s)=>Expr::Variable(s),
            TokenKind::LParen=>{let e=self.expr(0)?;self.expect(TokenKind::RParen)?;e},
            TokenKind::Plus=>Expr::Unary{op:UnaryOp::Plus,expr:Box::new(self.expr(19)?)},
            TokenKind::Minus=>Expr::Unary{op:UnaryOp::Minus,expr:Box::new(self.expr(19)?)},
            TokenKind::Tilde=>Expr::Unary{op:UnaryOp::BitNot,expr:Box::new(self.expr(19)?)},
            TokenKind::Bang=>Expr::Unary{op:UnaryOp::LogicalNot,expr:Box::new(self.expr(19)?)},
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
            if l<min { break; } self.take(); let rhs=self.expr(r)?;
            lhs=Expr::Binary { op, left:Box::new(lhs), right:Box::new(rhs) };
        }
        Ok(lhs)
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
}
