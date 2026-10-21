use crate::lexer::{Token,TokenKind};
#[derive(Debug,Clone,PartialEq)]pub enum Expr{Number(i64),Unary{op:UnaryOp,expr:Box<Expr>},Binary{op:BinaryOp,left:Box<Expr>,right:Box<Expr>}}
#[derive(Debug,Clone,Copy,PartialEq)]pub enum UnaryOp{Plus,Minus,BitNot}
#[derive(Debug,Clone,Copy,PartialEq)]pub enum BinaryOp{Add,Sub,Mul,Div,Rem,BitAnd,BitOr,BitXor}
pub struct Program{pub return_expr:Expr}
pub struct Parser{tokens:Vec<Token>,pos:usize}
impl Parser{
 pub fn new(tokens:Vec<Token>)->Self{Self{tokens,pos:0}}
 fn peek(&self)->&TokenKind{&self.tokens[self.pos].kind}fn take(&mut self)->Token{let t=self.tokens[self.pos].clone();self.pos+=1;t}
 fn expect(&mut self,w:TokenKind)->Result<(),String>{let t=self.take();if t.kind==w{Ok(())}else{Err(format!("at character {}: expected {w:?}, got {:?}",t.offset,t.kind))}}
 fn ident(&mut self,w:&str)->Result<(),String>{let t=self.take();if t.kind==TokenKind::Ident(w.into()){Ok(())}else{Err(format!("at character {}: expected identifier {w}, got {:?}",t.offset,t.kind))}}
 pub fn parse(mut self)->Result<Program,String>{self.ident("int")?;self.ident("main")?;self.expect(TokenKind::LParen)?;if *self.peek()==TokenKind::Ident("void".into()){self.take();}self.expect(TokenKind::RParen)?;self.expect(TokenKind::LBrace)?;self.ident("return")?;let expr=self.expression(0)?;self.expect(TokenKind::Semicolon)?;self.expect(TokenKind::RBrace)?;if *self.peek()!=TokenKind::End{return Err(format!("unexpected token {:?}",self.peek()))}Ok(Program{return_expr:expr})}
 fn expression(&mut self,min_bp:u8)->Result<Expr,String>{let t=self.take();let mut lhs=match t.kind{TokenKind::Number(n)=>Expr::Number(n),TokenKind::LParen=>{let e=self.expression(0)?;self.expect(TokenKind::RParen)?;e},TokenKind::Plus=>Expr::Unary{op:UnaryOp::Plus,expr:Box::new(self.expression(11)?)},TokenKind::Minus=>Expr::Unary{op:UnaryOp::Minus,expr:Box::new(self.expression(11)?)},TokenKind::Tilde=>Expr::Unary{op:UnaryOp::BitNot,expr:Box::new(self.expression(11)?)},other=>return Err(format!("at character {}: expected expression, got {other:?}",t.offset))};
 loop{let (op,l_bp,r_bp)=match self.peek(){TokenKind::Pipe=>(BinaryOp::BitOr,1,2),TokenKind::Caret=>(BinaryOp::BitXor,3,4),TokenKind::Amp=>(BinaryOp::BitAnd,5,6),TokenKind::Plus=>(BinaryOp::Add,7,8),TokenKind::Minus=>(BinaryOp::Sub,7,8),TokenKind::Star=>(BinaryOp::Mul,9,10),TokenKind::Slash=>(BinaryOp::Div,9,10),TokenKind::Percent=>(BinaryOp::Rem,9,10),_=>(break)};if l_bp<min_bp{break}self.take();let rhs=self.expression(r_bp)?;lhs=Expr::Binary{op,left:Box::new(lhs),right:Box::new(rhs)};}Ok(lhs)
 }
}
#[cfg(test)]mod tests{use super::*;use crate::lexer::lex;fn parse(s:&str)->Program{Parser::new(lex(s).unwrap()).parse().unwrap()}#[test]fn parses_return(){assert_eq!(parse("int main(void){return 42;}").return_expr,Expr::Number(42))}#[test]fn precedence_mul_before_add(){assert_eq!(parse("int main(){return 1+2*3;}").return_expr,Expr::Binary{op:BinaryOp::Add,left:Box::new(Expr::Number(1)),right:Box::new(Expr::Binary{op:BinaryOp::Mul,left:Box::new(Expr::Number(2)),right:Box::new(Expr::Number(3))})})}#[test]fn rejects_unknown_identifier(){assert!(Parser::new(lex("int main(){return x;}").unwrap()).parse().is_err())}}
