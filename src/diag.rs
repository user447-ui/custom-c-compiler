//! Source positions and diagnostics.
//!
//! Every stage reports problems as a [`Diagnostic`]: a message and, when the stage knows where
//! the problem is, a [`Pos`]. Only `main` renders them, with [`SourceMap::render`], so the text
//! of a message never has to mention a position itself.

use std::fmt;

/// A place in the source: 1-based line and 1-based column, counted in characters (a tab is one
/// column, as in Clang). `Pos::default()` (line 0) means "no position".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos { pub line: u32, pub col: u32 }

impl Pos {
    pub fn is_known(self) -> bool { self.line > 0 }
}

/// Printed as `line:col`, which keeps `--dump-ast` readable.
impl fmt::Debug for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}:{}", self.line, self.col) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic { pub pos: Option<Pos>, pub message: String }

impl Diagnostic {
    /// A diagnostic about a place in the source. An unknown position is treated as none.
    pub fn at(pos: Pos, message: impl Into<String>) -> Self {
        Self { pos: pos.is_known().then_some(pos), message: message.into() }
    }
    #[cfg(test)]
    pub fn contains(&self, text: &str) -> bool { self.message.contains(text) }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.message) }
}

/// A diagnostic about the program as a whole, such as a missing `main`.
impl From<String> for Diagnostic {
    fn from(message: String) -> Self { Self { pos: None, message } }
}
impl From<&str> for Diagnostic {
    fn from(message: &str) -> Self { Self { pos: None, message: message.to_string() } }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity { Error, Warning }

impl Severity {
    fn word(self) -> &'static str { match self { Severity::Error => "error", Severity::Warning => "warning" } }
}

/// The widest part of a source line that a diagnostic shows.
const SNIPPET_WIDTH: usize = 160;

/// The text of one source file, for turning [`Diagnostic`]s into the familiar
/// `file:line:col: error: message` followed by the offending line and a caret.
pub struct SourceMap<'a> { file: &'a str, lines: Vec<&'a str> }

impl<'a> SourceMap<'a> {
    pub fn new(file: &'a str, source: &'a str) -> Self {
        // Split on '\n' only, as the lexer does (columns are counted from the last '\n'); a
        // trailing '\r' of a CRLF line ending is not part of the line.
        let lines = source.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
        Self { file, lines }
    }

    pub fn render(&self, severity: Severity, d: &Diagnostic) -> String {
        let Some(pos) = d.pos else { return format!("ccc: {}: {}", severity.word(), d.message) };
        let mut out = format!("{}:{}:{}: {}: {}", self.file, pos.line, pos.col, severity.word(), d.message);
        let text: Vec<char> = self.lines.get(pos.line as usize - 1).copied().unwrap_or("").chars().collect();
        let column = pos.col as usize - 1;
        // Nothing useful to show for a position past the end of its line (end of input).
        if !text.iter().all(|c| c.is_whitespace()) && column <= text.len() {
            // A minified file can be one enormous line: show only a window around the error.
            let (start, end) = if text.len() <= SNIPPET_WIDTH { (0, text.len()) } else {
                let start = column.saturating_sub(SNIPPET_WIDTH / 2).min(text.len() - SNIPPET_WIDTH);
                (start, start + SNIPPET_WIDTH)
            };
            let (before, after) = (if start > 0 { "..." } else { "" }, if end < text.len() { "..." } else { "" });
            out.push('\n');
            out.push_str(before);
            out.extend(&text[start..end]);
            out.push_str(after);
            out.push('\n');
            // Copy the tabs of the source line so that the caret lines up under any tab width.
            out.push_str(&" ".repeat(before.len()));
            out.extend(text[start..column].iter().map(|c| if *c == '\t' { '\t' } else { ' ' }));
            out.push('^');
        }
        out
    }
}

/// For tests: the position of the `nth` (1-based) occurrence of `needle` in `source`, so that
/// expectations are found by searching rather than by counting columns by hand.
#[cfg(test)]
pub fn locate(source: &str, needle: &str, nth: usize) -> Pos {
    let mut at = None;
    for _ in 0..nth { at = Some(source[at.map_or(0, |i| i + 1)..].find(needle).expect("needle not found") + at.map_or(0, |i| i + 1)); }
    let byte = at.unwrap();
    let before = &source[..byte];
    Pos { line: before.matches('\n').count() as u32 + 1, col: before.rsplit('\n').next().unwrap().chars().count() as u32 + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, col: u32) -> Pos { Pos { line, col } }
    fn render(source: &str, d: &Diagnostic) -> String { SourceMap::new("t.c", source).render(Severity::Error, d) }

    #[test] fn renders_location_line_and_caret() {
        let source = "int main(void) {\n    return 1 +;\n}\n";
        let d = Diagnostic::at(pos(2, 15), "expected expression, got ';'");
        assert_eq!(render(source, &d),
            "t.c:2:15: error: expected expression, got ';'\n    return 1 +;\n              ^");
    }
    #[test] fn warnings_are_labelled_as_such() {
        let d = Diagnostic::at(pos(1, 1), "careful");
        assert!(SourceMap::new("a.c", "x").render(Severity::Warning, &d).starts_with("a.c:1:1: warning: careful"));
    }
    #[test] fn a_diagnostic_without_a_position_has_no_location() {
        assert_eq!(render("int x;", &Diagnostic::from("program has no 'main' function")),
            "ccc: error: program has no 'main' function");
        assert_eq!(Diagnostic::at(Pos::default(), "x").pos, None);
    }
    #[test] fn the_caret_copies_tabs_so_it_lines_up() {
        let d = Diagnostic::at(pos(1, 3), "here");
        assert_eq!(render("\t\tfoo bar", &d), "t.c:1:3: error: here\n\t\tfoo bar\n\t\t^");
        let d = Diagnostic::at(pos(1, 6), "here");
        assert_eq!(render("\tint x;", &d), "t.c:1:6: error: here\n\tint x;\n\t    ^"); // tab, then 4 spaces
    }
    #[test] fn columns_count_characters_not_bytes() {
        // 'é' is two bytes but one column: the caret goes under the `x`.
        let d = Diagnostic::at(pos(1, 4), "here");
        assert_eq!(render("é; x", &d), "t.c:1:4: error: here\né; x\n   ^");
    }
    #[test] fn crlf_line_endings_do_not_leak_into_the_snippet() {
        let d = Diagnostic::at(pos(2, 1), "here");
        assert_eq!(render("int a;\r\nint b;\r\n", &d), "t.c:2:1: error: here\nint b;\n^");
    }
    #[test] fn end_of_input_shows_the_location_only() {
        // A position on the empty line after the last newline, or past the file: no snippet.
        let d = Diagnostic::at(pos(3, 1), "unexpected end of input");
        assert_eq!(render("int main(void) {\n  return 0;\n", &d), "t.c:3:1: error: unexpected end of input");
        let d = Diagnostic::at(pos(9, 9), "gone");
        assert_eq!(render("x", &d), "t.c:9:9: error: gone");
    }
    #[test] fn a_position_just_past_the_last_character_gets_a_caret() {
        // `int x` with the `;` missing: the error is reported after the `x`.
        let d = Diagnostic::at(pos(1, 6), "expected ';'");
        assert_eq!(render("int x", &d), "t.c:1:6: error: expected ';'\nint x\n     ^");
    }
    #[test] fn a_very_long_line_is_shown_as_a_window_around_the_error() {
        let line = format!("{}@{}", "a ".repeat(500), " b".repeat(500));
        let col = line.chars().position(|c| c == '@').unwrap() as u32 + 1;
        let shown = render(&line, &Diagnostic::at(pos(1, col), "bad"));
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("...") && lines[1].ends_with("..."));
        assert!(lines[1].chars().count() <= SNIPPET_WIDTH + 6, "{}", lines[1].len());
        // the caret is under the `@`
        assert_eq!(lines[1].chars().nth(lines[2].chars().count() - 1), Some('@'));
        // near the start or the end only the far side is cut
        let at_start = render(&line, &Diagnostic::at(pos(1, 3), "bad"));
        assert!(!at_start.lines().nth(1).unwrap().starts_with("...") && at_start.lines().nth(1).unwrap().ends_with("..."));
        let last = line.chars().count() as u32;
        let at_end = render(&line, &Diagnostic::at(pos(1, last), "bad"));
        assert!(at_end.lines().nth(1).unwrap().starts_with("...") && !at_end.lines().nth(1).unwrap().ends_with("..."));
        assert_eq!(at_end.lines().nth(2).unwrap().chars().count(), at_end.lines().nth(1).unwrap().chars().count());
    }
    #[test] fn a_line_that_just_fits_is_shown_whole() {
        let line = "x".repeat(SNIPPET_WIDTH);
        let shown = render(&line, &Diagnostic::at(pos(1, 5), "bad"));
        assert_eq!(shown.lines().nth(1), Some(line.as_str()));
    }
}
