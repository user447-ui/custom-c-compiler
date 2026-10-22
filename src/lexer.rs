#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String), Number(i64),
    LParen, RParen, LBrace, RBrace, Semicolon, Comma,
    Equal, EqualEqual, Bang, BangEqual,
    Less, LessEqual, Greater, GreaterEqual,
    Amp, AmpAmp, Pipe, PipePipe, Caret, Tilde,
    Plus, Minus, Star, Slash, Percent, End, Other(char),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Token { pub kind: TokenKind, pub offset: usize }

pub fn lex(source: &str) -> Result<Vec<Token>, String> {
    let c: Vec<char> = source.chars().collect();
    let (mut i, mut out) = (0, Vec::new());
    while i < c.len() {
        let x = c[i];
        if x.is_whitespace() { i += 1; continue; }
        if x == '/' && i + 1 < c.len() && c[i + 1] == '/' {
            i += 2; while i < c.len() && c[i] != '\n' { i += 1; } continue;
        }
        if x == '/' && i + 1 < c.len() && c[i + 1] == '*' {
            let start = i; i += 2; let mut done = false;
            while i + 1 < c.len() {
                if c[i] == '*' && c[i + 1] == '/' { i += 2; done = true; break; }
                i += 1;
            }
            if !done { return Err(format!("unterminated block comment at character {start}")); }
            continue;
        }
        let start = i;
        if x.is_ascii_alphabetic() || x == '_' {
            i += 1;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') { i += 1; }
            out.push(Token { kind: TokenKind::Ident(c[start..i].iter().collect()), offset: start });
            continue;
        }
        if x.is_ascii_digit() {
            i += 1; while i < c.len() && c[i].is_ascii_digit() { i += 1; }
            let n: String = c[start..i].iter().collect();
            out.push(Token { kind: TokenKind::Number(n.parse().map_err(|_| format!("integer literal out of range at character {start}"))?), offset: start });
            continue;
        }
        let pair = if i + 1 < c.len() { Some((x, c[i + 1])) } else { None };
        let two = match pair {
            Some(('=', '=')) => Some(TokenKind::EqualEqual),
            Some(('!', '=')) => Some(TokenKind::BangEqual),
            Some(('<', '=')) => Some(TokenKind::LessEqual),
            Some(('>', '=')) => Some(TokenKind::GreaterEqual),
            Some(('&', '&')) => Some(TokenKind::AmpAmp),
            Some(('|', '|')) => Some(TokenKind::PipePipe),
            _ => None,
        };
        if let Some(kind) = two { out.push(Token { kind, offset: start }); i += 2; continue; }
        let kind = match x {
            '(' => TokenKind::LParen, ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace, '}' => TokenKind::RBrace, ';' => TokenKind::Semicolon, ',' => TokenKind::Comma,
            '=' => TokenKind::Equal, '!' => TokenKind::Bang,
            '<' => TokenKind::Less, '>' => TokenKind::Greater,
            '&' => TokenKind::Amp, '|' => TokenKind::Pipe, '^' => TokenKind::Caret, '~' => TokenKind::Tilde,
            '+' => TokenKind::Plus, '-' => TokenKind::Minus, '*' => TokenKind::Star,
            '/' => TokenKind::Slash, '%' => TokenKind::Percent, z => TokenKind::Other(z),
        };
        out.push(Token { kind, offset: start }); i += 1;
    }
    out.push(Token { kind: TokenKind::End, offset: source.len() });
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn comments_and_assignment() { assert!(lex("int x=1; // ok\n").unwrap().iter().any(|t| t.kind == TokenKind::Equal)); }
    #[test] fn comparison_tokens() { let t=lex("a<=b && a!=0 || !b").unwrap(); assert!(t.iter().any(|x|x.kind==TokenKind::LessEqual)); assert!(t.iter().any(|x|x.kind==TokenKind::AmpAmp)); assert!(t.iter().any(|x|x.kind==TokenKind::Bang)); }
    #[test] fn comma_token() { assert!(lex("f(a,b)").unwrap().iter().any(|t| t.kind == TokenKind::Comma)); }
    #[test] fn bad_comment() { assert!(lex("/*").is_err()); }
}
