use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};
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
        let t=self.take(); if let TokenKind::Ident(s)=t.kind { Ok(s) } else { Err(format!("at character {}: expected identifier, got {:?}",t.offset,t.kind)) }
    }
    pub fn parse(mut self) -> Result<Program, String> {
        self.ident("int")?; self.ident("main")?; self.expect(TokenKind::LParen)?;
        if *self.peek()==TokenKind::Ident("void".into()) { self.take(); }
        self.expect(TokenKind::RParen)?; let statements=self.block()?;
        if *self.peek()!=TokenKind::End { return Err(format!("unexpected trailing token {:?}",self.peek())); }
        Ok(Program { statements })
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
            x => Err(format!("unexpected token at statement start: {x:?}")),
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let t=self.take();
        let mut lhs=match t.kind {
            TokenKind::Number(n)=>Expr::Number(n),
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
#[cfg(test)]
mod tests {
    use super::*; use crate::lexer::lex;
    #[test] fn parses_statements() { let p=Parser::new(lex("int main(){int x=1;while(x-5)x=x+1;return x;}").unwrap()).parse().unwrap(); assert_eq!(p.statements.len(),3); }
    #[test] fn parses_precedence() { let p=Parser::new(lex("int main(){return 1+2<4 && !0;}").unwrap()).parse().unwrap(); assert_eq!(p.statements.len(),1); }
    #[test] fn rejects_missing_semicolon() { assert!(Parser::new(lex("int main(){return 1}").unwrap()).parse().is_err()); }
}
