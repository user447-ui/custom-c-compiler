use crate::ast::{BinaryOp, Expr, Program, UnaryOp};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn take(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        self.pos += 1;
        token
    }

    fn expect(&mut self, wanted: TokenKind) -> Result<(), String> {
        let token = self.take();
        if token.kind == wanted {
            Ok(())
        } else {
            Err(format!("at character {}: expected {wanted:?}, got {:?}", token.offset, token.kind))
        }
    }

    fn ident(&mut self, wanted: &str) -> Result<(), String> {
        let token = self.take();
        if token.kind == TokenKind::Ident(wanted.into()) {
            Ok(())
        } else {
            Err(format!("at character {}: expected identifier {wanted}, got {:?}", token.offset, token.kind))
        }
    }

    pub fn parse(mut self) -> Result<Program, String> {
        self.ident("int")?;
        self.ident("main")?;
        self.expect(TokenKind::LParen)?;
        if *self.peek() == TokenKind::Ident("void".into()) {
            self.take();
        }
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::LBrace)?;
        self.ident("return")?;
        let return_expr = self.expression(0)?;
        self.expect(TokenKind::Semicolon)?;
        self.expect(TokenKind::RBrace)?;
        if *self.peek() != TokenKind::End {
            return Err(format!("unexpected trailing token {:?}", self.peek()));
        }
        Ok(Program { return_expr })
    }

    // Pratt parser: each operator has left/right binding powers to encode precedence.
    fn expression(&mut self, min_bp: u8) -> Result<Expr, String> {
        let token = self.take();
        let mut lhs = match token.kind {
            TokenKind::Number(n) => Expr::Number(n),
            TokenKind::LParen => {
                let expression = self.expression(0)?;
                self.expect(TokenKind::RParen)?;
                expression
            }
            TokenKind::Plus => Expr::Unary {
                op: UnaryOp::Plus,
                expr: Box::new(self.expression(11)?),
            },
            TokenKind::Minus => Expr::Unary {
                op: UnaryOp::Minus,
                expr: Box::new(self.expression(11)?),
            },
            TokenKind::Tilde => Expr::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(self.expression(11)?),
            },
            other => return Err(format!("at character {}: expected expression, got {other:?}", token.offset)),
        };

        loop {
            let (op, left_bp, right_bp) = match self.peek() {
                TokenKind::Pipe => (BinaryOp::BitOr, 1, 2),
                TokenKind::Caret => (BinaryOp::BitXor, 3, 4),
                TokenKind::Amp => (BinaryOp::BitAnd, 5, 6),
                TokenKind::Plus => (BinaryOp::Add, 7, 8),
                TokenKind::Minus => (BinaryOp::Sub, 7, 8),
                TokenKind::Star => (BinaryOp::Mul, 9, 10),
                TokenKind::Slash => (BinaryOp::Div, 9, 10),
                TokenKind::Percent => (BinaryOp::Rem, 9, 10),
                _ => break,
            };
            if left_bp < min_bp {
                break;
            }
            self.take();
            let rhs = self.expression(right_bp)?;
            lhs = Expr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse(source: &str) -> Program {
        Parser::new(lex(source).unwrap()).parse().unwrap()
    }

    #[test]
    fn parses_return() {
        assert_eq!(parse("int main(void){return 42;}").return_expr, Expr::Number(42));
    }

    #[test]
    fn multiplication_has_higher_precedence_than_addition() {
        assert_eq!(
            parse("int main(){return 1+2*3;}").return_expr,
            Expr::Binary {
                op: BinaryOp::Add,
                left: Box::new(Expr::Number(1)),
                right: Box::new(Expr::Binary {
                    op: BinaryOp::Mul,
                    left: Box::new(Expr::Number(2)),
                    right: Box::new(Expr::Number(3)),
                }),
            }
        );
    }

    #[test]
    fn rejects_unknown_identifier() {
        assert!(Parser::new(lex("int main(){return x;}").unwrap()).parse().is_err());
    }
}
