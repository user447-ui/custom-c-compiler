//! Warnings found on the resolved AST: code that is legal but almost certainly not what was
//! meant. (Warnings that need the optimizer, such as a division that always faults, are found
//! on the IR by `ir::warnings`.)
//!
//! * a local variable that is never used, or is assigned but never read;
//! * an expression statement whose value is thrown away and that does nothing else.
//!
//! The AST passed in has been through semantic analysis, so every name refers to one
//! declaration: a local is `name#N`, which makes "is this variable read anywhere" a set lookup.

use crate::ast::{Expr, Function, Program, Stmt};
use crate::diag::{Diagnostic, Pos};
use std::collections::HashSet;

/// The warnings for a whole program, function by function, in source order within a function
/// for statements and then declarations. (`main` sorts all warnings by position before showing them.)
pub fn warnings(program: &Program) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for function in &program.functions { check_function(function, &mut out); }
    out
}

struct Local { name: String, pos: Pos }

#[derive(Default)]
struct Usage {
    /// Locals in declaration order.
    locals: Vec<Local>,
    /// Names whose value is used somewhere, and names that are the target of an assignment statement.
    read: HashSet<String>,
    assigned: HashSet<String>,
    warnings: Vec<Diagnostic>,
}

fn check_function(function: &Function, out: &mut Vec<Diagnostic>) {
    let Some(body) = &function.body else { return };
    let mut usage = Usage::default();
    usage.statements(body);
    out.append(&mut usage.warnings);
    for local in &usage.locals {
        if usage.read.contains(&local.name) { continue; }
        let shown = source_name(&local.name);
        let message = if usage.assigned.contains(&local.name) {
            format!("variable '{shown}' is set but never used")
        } else {
            format!("unused variable '{shown}'")
        };
        out.push(Diagnostic::at(local.pos, message));
    }
}

/// `x#3` is the third declaration sema resolved; the programmer wrote `x`.
fn source_name(resolved: &str) -> &str { resolved.split('#').next().unwrap_or(resolved) }

impl Usage {
    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements { self.statement(statement); }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Declaration { name, initializer, pos } => {
                if let Some(e) = initializer { self.expr(e); }
                self.locals.push(Local { name: name.clone(), pos: *pos });
            }
            Stmt::Assignment { name, value, .. } => {
                self.expr(value);
                self.assigned.insert(name.clone());
            }
            Stmt::Return(e) => self.expr(e),
            Stmt::Expr(e) => {
                self.expr(e);
                self.value_discarded(e);
            }
            Stmt::Block(body) => self.statements(body),
            Stmt::If { condition, then_branch, else_branch } => {
                self.expr(condition);
                self.statement(then_branch);
                if let Some(other) = else_branch { self.statement(other); }
            }
            Stmt::While { condition, body } => {
                self.expr(condition);
                self.statement(body);
            }
        }
    }

    /// Record every variable the expression reads.
    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Number(..) => {}
            Expr::Variable(name, _) => { self.read.insert(name.clone()); }
            Expr::Unary { expr, .. } => self.expr(expr),
            Expr::Binary { left, right, .. } => { self.expr(left); self.expr(right); }
            Expr::Call { args, .. } => { for a in args { self.expr(a); } }
        }
    }

    /// `e;` as a statement: only a call is worth writing that way.
    fn value_discarded(&mut self, e: &Expr) {
        if matches!(e, Expr::Call { .. }) { return; }
        let message = if contains_call(e) {
            "the value of this expression is computed but never used (only its calls have an effect)"
        } else {
            "statement has no effect"
        };
        self.warnings.push(Diagnostic::at(position(e), message));
    }
}

fn contains_call(e: &Expr) -> bool {
    match e {
        Expr::Call { .. } => true,
        Expr::Number(..) | Expr::Variable(..) => false,
        Expr::Unary { expr, .. } => contains_call(expr),
        Expr::Binary { left, right, .. } => contains_call(left) || contains_call(right),
    }
}

/// Where to point at an expression: its operator, or the thing itself.
fn position(e: &Expr) -> Pos {
    match e {
        Expr::Number(_, pos) | Expr::Variable(_, pos) => *pos,
        Expr::Unary { pos, .. } | Expr::Binary { pos, .. } | Expr::Call { pos, .. } => *pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{diag::locate, lexer::lex, parser::Parser, sema};

    fn lint(source: &str) -> Vec<(Pos, String)> {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        warnings(&sema::analyze(&ast).unwrap()).into_iter().map(|d| (d.pos.expect("positioned"), d.message)).collect()
    }
    fn messages(source: &str) -> Vec<String> { lint(source).into_iter().map(|(_, m)| m).collect() }

    #[test] fn a_variable_that_is_never_mentioned_is_unused() {
        let source = "int main(void) {\n  int a = 1;\n  int b = 2;\n  return b;\n}";
        assert_eq!(lint(source), vec![(locate(source, "a = 1", 1), "unused variable 'a'".to_string())]);
        // with or without an initializer
        assert_eq!(messages("int main(void){ int a; return 0; }"), vec!["unused variable 'a'"]);
    }
    #[test] fn a_variable_that_is_only_assigned_is_set_but_never_used() {
        let source = "int main(void) {\n  int a = 0;\n  a = 5;\n  return 0;\n}";
        assert_eq!(lint(source), vec![(locate(source, "a = 0", 1), "variable 'a' is set but never used".to_string())]);
        assert_eq!(messages("int main(void){ int a; a = 1; a = 2; return 0; }"), vec!["variable 'a' is set but never used"]);
    }
    #[test] fn any_read_counts_as_a_use() {
        for body in [
            "int a = 1; return a;",
            "int a = 1; if (a) { return 1; } return 0;",
            "int a = 3; while (a) { a = a - 1; } return 0;",
            "int a = 1; int b = a + 1; return b;",
            "int a = 1; return g(a);",
            "int a = 1; a = a + 1; return a;",
            "int a = 1; a = a + 1; return 0;", // read to compute itself: like GCC, not flagged
            "int a = 1; -a; return 0;",         // (this one has another warning, checked below)
        ] {
            let source = format!("int g(int x){{ return x; }} int main(void){{ {body} }}");
            assert!(!messages(&source).iter().any(|m| m.contains("variable")), "{body}: {:?}", messages(&source));
        }
    }
    #[test] fn shadowed_and_scoped_variables_are_tracked_separately() {
        let source = "int main(void) {\n  int x = 1;\n  {\n    int x = 2;\n  }\n  return x;\n}";
        // the inner x is unused although the outer x, with the same name, is used
        assert_eq!(lint(source), vec![(locate(source, "x = 2", 1), "unused variable 'x'".to_string())]);
        let source = "int main(void){ int i = 0; while (i < 3) { int step = 1; i = i + step; } return i; }";
        assert!(lint(source).is_empty());
    }
    #[test] fn parameters_are_never_reported() {
        assert!(lint("int f(int a, int b){ return 0; } int main(void){ return f(1, 2); }").is_empty());
    }
    #[test] fn unused_variables_are_reported_in_declaration_order_in_every_function() {
        let source = "int f(void){ int p = 1; int q = 2; return 0; }\nint main(void){ int r = 3; return f(); }";
        let w = lint(source);
        assert_eq!(w.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>(),
            ["unused variable 'p'", "unused variable 'q'", "unused variable 'r'"]);
        assert_eq!(w[2].0, locate(source, "r = 3", 1));
    }
    #[test] fn an_expression_statement_that_does_nothing_is_reported_at_its_operator() {
        let source = "int main(void) {\n  int a = 1;\n  int b = 2;\n  a + b;\n  a;\n  7;\n  -a;\n  !b;\n  return 0;\n}";
        let w = lint(source);
        assert_eq!(w, vec![
            (locate(source, "+ b", 1), "statement has no effect".to_string()),
            (locate(source, "a;\n  7", 1), "statement has no effect".to_string()),
            (locate(source, "7;", 1), "statement has no effect".to_string()),
            (locate(source, "-a", 1), "statement has no effect".to_string()),
            (locate(source, "!b", 1), "statement has no effect".to_string()),
        ]);
    }
    #[test] fn a_call_is_a_fine_statement_but_an_unused_result_computed_from_calls_is_not() {
        let source = "int g(int x){ return x; }\nint main(void) {\n  g(1);\n  g(2) + 1;\n  -g(3);\n  return 0;\n}";
        let w = lint(source);
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(w[0].0, locate(source, "+ 1", 1));
        assert_eq!(w[1].0, locate(source, "-g(3", 1));
        assert!(w.iter().all(|(_, m)| m.contains("computed but never used")), "{w:?}");
    }
    #[test] fn warnings_are_found_in_nested_statements() {
        let source = "int main(void) {\n  int n = 1;\n  while (n) {\n    if (n) {\n      n;\n    }\n    n = 0;\n  }\n  return 0;\n}";
        assert_eq!(lint(source), vec![(locate(source, "n;\n    }", 1), "statement has no effect".to_string())]);
    }
    #[test] fn clean_programs_have_no_warnings() {
        assert!(lint("int gcd(int a, int b){ while (b) { int t = b; b = a % b; a = t; } return a; } int main(void){ return gcd(12, 18); }").is_empty());
        assert!(lint("int main(void){ return 0; }").is_empty());
    }
    #[test] fn a_declaration_whose_initializer_reads_another_variable_uses_it() {
        assert!(lint("int main(void){ int a = 4; int b = a * 2; return b; }").is_empty());
    }
}
