#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind { Ident(String), Number(i64), LParen, RParen, LBrace, RBrace, Semicolon, Plus, Minus, Star, Slash, Percent, Amp, Pipe, Caret, Tilde, End, Other(char) }
#[derive(Debug, Clone, PartialEq)]
pub struct Token { pub kind: TokenKind, pub offset: usize }
pub fn lex(source: &str) -> Result<Vec<Token>, String> {
 let chars: Vec<char> = source.chars().collect(); let mut i=0; let mut tokens=Vec::new();
 while i<chars.len() { let c=chars[i]; if c.is_whitespace(){i+=1;continue}
 if c=='/' && i+1<chars.len() && chars[i+1]=='/' {i+=2;while i<chars.len()&&chars[i]!='\n'{i+=1}continue}
 if c=='/' && i+1<chars.len() && chars[i+1]=='*' {let start=i;i+=2;let mut closed=false;while i+1<chars.len(){if chars[i]=='*'&&chars[i+1]=='/'{i+=2;closed=true;break}i+=1}if !closed{return Err(format!("unterminated block comment at character {start}"))}continue}
 let start=i;
 if c.is_ascii_alphabetic()||c=='_' {i+=1;while i<chars.len()&&(chars[i].is_ascii_alphanumeric()||chars[i]=='_'){i+=1}tokens.push(Token{kind:TokenKind::Ident(chars[start..i].iter().collect()),offset:start});continue}
 if c.is_ascii_digit(){i+=1;while i<chars.len()&&chars[i].is_ascii_digit(){i+=1}let n:String=chars[start..i].iter().collect();tokens.push(Token{kind:TokenKind::Number(n.parse().map_err(|_|format!("integer literal out of range at character {start}"))?),offset:start});continue}
 let kind=match c{'('=>TokenKind::LParen,')'=>TokenKind::RParen,'{'=>TokenKind::LBrace,'}'=>TokenKind::RBrace,';'=>TokenKind::Semicolon,'+'=>TokenKind::Plus,'-'=>TokenKind::Minus,'*'=>TokenKind::Star,'/'=>TokenKind::Slash,'%'=>TokenKind::Percent,'&'=>TokenKind::Amp,'|'=>TokenKind::Pipe,'^'=>TokenKind::Caret,'~'=>TokenKind::Tilde,x=>TokenKind::Other(x)};tokens.push(Token{kind,offset:i});i+=1;
 }
 tokens.push(Token{kind:TokenKind::End,offset:source.len()});Ok(tokens)
}
#[cfg(test)]mod tests{use super::*;#[test]fn tokenizes_operators(){let t=lex("1 + 2 * 3").unwrap();assert!(t.iter().any(|x|x.kind==TokenKind::Plus));assert!(t.iter().any(|x|x.kind==TokenKind::Star));}#[test]fn rejects_unclosed_comment(){assert!(lex("/* no end").is_err())}}
