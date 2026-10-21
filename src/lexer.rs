#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind { Ident(String), Number(i64), LParen, RParen, LBrace, RBrace, Semicolon, Equal, Plus, Minus, Star, Slash, Percent, Amp, Pipe, Caret, Tilde, End, Other(char) }
#[derive(Debug, Clone, PartialEq)]
pub struct Token { pub kind: TokenKind, pub offset: usize }
pub fn lex(source: &str) -> Result<Vec<Token>, String> {
 let c: Vec<char> = source.chars().collect(); let (mut i, mut out) = (0, Vec::new());
 while i < c.len() { let x=c[i]; if x.is_whitespace(){i+=1;continue}
 if x=='/' && i+1<c.len() && c[i+1]=='/' {i+=2;while i<c.len()&&c[i]!='\n'{i+=1}continue}
 if x=='/' && i+1<c.len() && c[i+1]=='*' {let s=i;i+=2;let mut done=false;while i+1<c.len(){if c[i]=='*'&&c[i+1]=='/'{i+=2;done=true;break}i+=1}if !done{return Err(format!("unterminated block comment at character {s}"))}continue}
 let s=i; if x.is_ascii_alphabetic()||x=='_' {i+=1;while i<c.len()&&(c[i].is_ascii_alphanumeric()||c[i]=='_'){i+=1}out.push(Token{kind:TokenKind::Ident(c[s..i].iter().collect()),offset:s});continue}
 if x.is_ascii_digit(){i+=1;while i<c.len()&&c[i].is_ascii_digit(){i+=1}let n:String=c[s..i].iter().collect();out.push(Token{kind:TokenKind::Number(n.parse().map_err(|_|format!("integer literal out of range at character {s}"))?),offset:s});continue}
 let k=match x{'('=>TokenKind::LParen,')'=>TokenKind::RParen,'{'=>TokenKind::LBrace,'}'=>TokenKind::RBrace,';'=>TokenKind::Semicolon,'='=>TokenKind::Equal,'+'=>TokenKind::Plus,'-'=>TokenKind::Minus,'*'=>TokenKind::Star,'/'=>TokenKind::Slash,'%'=>TokenKind::Percent,'&'=>TokenKind::Amp,'|'=>TokenKind::Pipe,'^'=>TokenKind::Caret,'~'=>TokenKind::Tilde,z=>TokenKind::Other(z)};out.push(Token{kind:k,offset:i});i+=1;
 } out.push(Token{kind:TokenKind::End,offset:source.len()});Ok(out)
}
#[cfg(test)] mod tests { use super::*; #[test] fn comments_and_assignment(){assert!(lex("int x=1; // ok\n").unwrap().iter().any(|t|t.kind==TokenKind::Equal));} #[test] fn bad_comment(){assert!(lex("/*").is_err());} }
