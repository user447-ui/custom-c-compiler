use std::collections::{HashMap, HashSet};
use crate::ast::{BinaryOp, Expr, Program, Stmt};

/// Resolve identifiers and validate declarations while preserving lexical scope,
/// then reject reads of locals that are not definitely initialized.
pub fn analyze(program: &Program) -> Result<Program, String> {
    let mut analyzer = Analyzer { scopes: vec![HashMap::new()], next_id: 0 };
    let resolved = Program { statements: analyzer.statements(&program.statements)? };
    let mut flow = Flow { initialized: HashSet::new() };
    flow.statements(&resolved.statements)?;
    Ok(resolved)
}

struct Analyzer { scopes: Vec<HashMap<String, String>>, next_id: usize }
impl Analyzer {
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self) { self.scopes.pop(); }
    fn resolve(&self, name: &str) -> Result<String, String> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
            .ok_or_else(|| format!("use of undeclared variable '{name}'"))
    }
    fn statements(&mut self, ss: &[Stmt]) -> Result<Vec<Stmt>, String> {
        ss.iter().map(|s| self.statement(s)).collect()
    }
    fn scoped(&mut self, s: &Stmt) -> Result<Stmt, String> {
        self.push(); let result = self.statement(s); self.pop(); result
    }
    fn statement(&mut self, s: &Stmt) -> Result<Stmt, String> {
        match s {
            Stmt::Declaration { name, initializer } => {
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(format!("redeclaration of local variable '{name}' in the same scope"));
                }
                let initializer = initializer.as_ref().map(|e| self.expr(e)).transpose()?;
                let unique = format!("{name}#{}", self.next_id); self.next_id += 1;
                self.scopes.last_mut().unwrap().insert(name.clone(), unique.clone());
                Ok(Stmt::Declaration { name: unique, initializer })
            }
            Stmt::Assignment { name, value } => {
                let name = self.resolve(name)?; let value = self.expr(value)?;
                Ok(Stmt::Assignment { name, value })
            }
            Stmt::Return(e) => Ok(Stmt::Return(self.expr(e)?)),
            Stmt::Block(body) => {
                self.push(); let body = self.statements(body); self.pop();
                Ok(Stmt::Block(body?))
            }
            Stmt::If { condition, then_branch, else_branch } => {
                let condition = self.expr(condition)?;
                let then_branch = Box::new(self.scoped(then_branch)?);
                let else_branch = else_branch.as_ref().map(|s| self.scoped(s).map(Box::new)).transpose()?;
                Ok(Stmt::If { condition, then_branch, else_branch })
            }
            Stmt::While { condition, body } => {
                let condition = self.expr(condition)?;
                Ok(Stmt::While { condition, body: Box::new(self.scoped(body)?) })
            }
        }
    }
    fn expr(&self, e: &Expr) -> Result<Expr, String> {
        Ok(match e {
            Expr::Number(n) => Expr::Number(*n),
            Expr::Variable(n) => Expr::Variable(self.resolve(n)?),
            Expr::Unary { op, expr } => Expr::Unary { op: *op, expr: Box::new(self.expr(expr)?) },
            Expr::Binary { op, left, right } => Expr::Binary {
                op: *op, left: Box::new(self.expr(left)?), right: Box::new(self.expr(right)?)
            },
        })
    }
}

struct Flow { initialized: HashSet<String> }
impl Flow {
    fn statements(&mut self, ss: &[Stmt]) -> Result<(), String> {
        for s in ss { self.statement(s)?; }
        Ok(())
    }

    fn statement(&mut self, s: &Stmt) -> Result<(), String> {
        match s {
            Stmt::Declaration { name, initializer } => {
                // The initializer is evaluated before this declaration becomes initialized.
                self.initialized.remove(name);
                if let Some(e) = initializer { self.expr(e)?; self.initialized.insert(name.clone()); }
            }
            Stmt::Assignment { name, value } => {
                self.expr(value)?;
                self.initialized.insert(name.clone());
            }
            Stmt::Return(e) => self.expr(e)?,
            Stmt::Block(body) => self.statements(body)?,
            Stmt::If { condition, then_branch, else_branch } => {
                self.expr(condition)?;
                let incoming = self.initialized.clone();
                let mut then_flow = Flow { initialized: incoming.clone() };
                then_flow.statement(then_branch)?;
                let mut else_flow = Flow { initialized: incoming.clone() };
                if let Some(other) = else_branch {
                    else_flow.statement(other)?;
                }
                self.initialized = then_flow.initialized.intersection(&else_flow.initialized)
                    .cloned().collect();
            }
            Stmt::While { condition, body } => {
                self.expr(condition)?;
                // A loop may execute zero times, so body assignments cannot establish
                // definite initialization after the loop. Still validate reads in it.
                let mut body_flow = Flow { initialized: self.initialized.clone() };
                body_flow.statement(body)?;
            }
        }
        Ok(())
    }

    fn expr(&self, e: &Expr) -> Result<(), String> {
        match e {
            Expr::Number(_) => Ok(()),
            Expr::Variable(name) => {
                if self.initialized.contains(name) { Ok(()) }
                else {
                    let source_name = name.split('#').next().unwrap_or(name);
                    Err(format!("read of uninitialized variable '{source_name}'"))
                }
            }
            Expr::Unary { expr, .. } => self.expr(expr),
            Expr::Binary { op, left, right } => {
                self.expr(left)?;
                // For logical operators the RHS may not execute, but this language
                // has no assignment expressions; checking it is conservative and
                // catches reads that could occur when the RHS is evaluated.
                let _ = op == &BinaryOp::LogicalAnd || op == &BinaryOp::LogicalOr;
                self.expr(right)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::Parser};
    fn run(s: &str) -> Result<Program, String> { analyze(&Parser::new(lex(s)?).parse()?) }

    #[test] fn rejects_use_before_declaration() {
        assert!(run("int main(){ x=2; int x; return 0; }").unwrap_err().contains("undeclared variable 'x'"));
    }
    #[test] fn rejects_same_scope_redeclaration() {
        assert!(run("int main(){ int x; int x; return 0; }").unwrap_err().contains("redeclaration"));
    }
    #[test] fn permits_shadowing() {
        let p=run("int main(){ int x=3; { int x=7; x=x+1; } return x; }").unwrap();
        let Stmt::Declaration{name:outer,..}=&p.statements[0] else { panic!() };
        let Stmt::Block(inner)=&p.statements[1] else { panic!() };
        let Stmt::Declaration{name:inner,..}=&inner[0] else { panic!() };
        assert_ne!(outer,inner);
    }
    #[test] fn resolves_outer_variable() { assert!(run("int main(){ int x=3; { int y=x; } return x; }").is_ok()); }
    #[test] fn rejects_uninitialized_read() {
        assert!(run("int main(){ int x; return x; }").unwrap_err().contains("uninitialized variable 'x'"));
    }
    #[test] fn accepts_assignment_before_read() {
        assert!(run("int main(){ int x; x=2; return x; }").is_ok());
    }
    #[test] fn if_requires_initialization_on_both_paths() {
        assert!(run("int main(){ int x; if(1) x=3; return x; }").is_err());
        assert!(run("int main(){ int x; if(1) x=3; else x=4; return x; }").is_ok());
    }
    #[test] fn loop_body_does_not_initialize_after_loop() {
        assert!(run("int main(){ int x; while(0) x=4; return x; }").is_err());
    }
    #[test] fn initializer_cannot_read_same_uninitialized_variable() {
        assert!(run("int main(){ int x; { int x=x; return x; } return 0; }").is_err());
    }
}
