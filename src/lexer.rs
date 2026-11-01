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
            // Take the whole alphanumeric run so that `12abc`, `0x` and `08` are
            // diagnosed as one bad literal instead of splitting into several tokens.
            i += 1;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') { i += 1; }
            let text: String = c[start..i].iter().collect();
            let value = lex_number(&text).map_err(|e| format!("{e} at character {start}"))?;
            out.push(Token { kind: TokenKind::Number(value), offset: start });
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

/// Value of an integer literal: decimal (`42`), octal (`052`) or hexadecimal (`0x2A`).
///
/// Decimal literals are only checked against `i64` here; `sema` applies the `int`
/// range (so that `-2147483648` can be accepted). Octal and hexadecimal literals that
/// do not fit in `int` have type `unsigned int` or wider in C, which this compiler
/// does not have yet, so they are rejected here instead of being silently reinterpreted.
fn lex_number(text: &str) -> Result<i64, String> {
    let (radix, digits, what) = match text.as_bytes() {
        [b'0', b'x' | b'X', ..] => (16, &text[2..], "hexadecimal"),
        [b'0', _, ..] => (8, &text[1..], "octal"),
        _ => (10, text, "decimal"),
    };
    if digits.is_empty() { return Err(format!("{what} literal '{text}' has no digits")); }
    if let Some(bad) = digits.chars().find(|d| !d.is_digit(radix)) {
        return Err(if bad.is_ascii_digit() {
            format!("invalid digit '{bad}' in {what} literal '{text}'")
        } else {
            format!("invalid suffix or character '{bad}' in integer literal '{text}' (suffixes are not supported)")
        });
    }
    let value = u64::from_str_radix(digits, radix)
        .ok().and_then(|v| i64::try_from(v).ok())
        .ok_or_else(|| "integer literal out of range".to_string())?;
    if radix != 10 && value > i64::from(i32::MAX) {
        return Err(format!("{what} literal '{text}' does not fit in 'int' (unsigned types are not supported yet)"));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn comments_and_assignment() { assert!(lex("int x=1; // ok\n").unwrap().iter().any(|t| t.kind == TokenKind::Equal)); }
    #[test] fn comparison_tokens() { let t=lex("a<=b && a!=0 || !b").unwrap(); assert!(t.iter().any(|x|x.kind==TokenKind::LessEqual)); assert!(t.iter().any(|x|x.kind==TokenKind::AmpAmp)); assert!(t.iter().any(|x|x.kind==TokenKind::Bang)); }
    #[test] fn comma_token() { assert!(lex("f(a,b)").unwrap().iter().any(|t| t.kind == TokenKind::Comma)); }
    #[test] fn bad_comment() { assert!(lex("/*").is_err()); }
    fn num(s: &str) -> Result<i64, String> {
        lex(s).map(|t| match t[0].kind { TokenKind::Number(n) => n, ref k => panic!("{k:?}") })
    }
    #[test] fn decimal_octal_and_hex_literals() {
        assert_eq!(num("0"), Ok(0));
        assert_eq!(num("42"), Ok(42));
        assert_eq!(num("010"), Ok(8));
        assert_eq!(num("0777"), Ok(511));
        assert_eq!(num("0x10"), Ok(16));
        assert_eq!(num("0XfF"), Ok(255));
        assert_eq!(num("0x7fffffff"), Ok(2147483647));
        assert_eq!(num("017777777777"), Ok(2147483647));
        assert_eq!(num("2147483648"), Ok(2147483648)); // range-checked by sema, for -2147483648
    }
    #[test] fn malformed_literals_are_rejected() {
        for bad in ["08", "0189", "0x", "0X", "0xg", "0x1fz", "12abc", "1_000", "10u", "5L", "0b101"] {
            assert!(num(bad).is_err(), "{bad} should be rejected");
        }
        assert!(num("08").unwrap_err().contains("invalid digit '8' in octal"));
        assert!(num("0x").unwrap_err().contains("no digits"));
        assert!(num("12abc").unwrap_err().contains("invalid suffix"));
    }
    #[test] fn non_decimal_literals_that_do_not_fit_int_are_rejected() {
        for bad in ["0x80000000", "0xffffffff", "020000000000", "0x100000000", "0xffffffffffffffffff"] {
            assert!(num(bad).is_err(), "{bad} should be rejected");
        }
        assert!(num("0x80000000").unwrap_err().contains("does not fit in 'int'"));
    }
    #[test] fn literal_followed_by_operator_still_splits() {
        let t = lex("0x10+010").unwrap();
        assert_eq!((t[0].kind.clone(), t[2].kind.clone()), (TokenKind::Number(16), TokenKind::Number(8)));
    }
}
