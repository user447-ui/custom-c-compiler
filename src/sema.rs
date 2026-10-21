use std::collections::HashMap;
use crate::ast::{Expr, Program, Stmt};

/// Resolve identifiers and validate declarations while preserving lexical scope.
pub fn analyze(program: &Program) -> Result<Program, String> {
    let mut analyzer = Analyzer { scopes: vec![HashMap::new()], next_id: 0 };
    Ok(Program { statements: analyzer.statements(&program.statements)? })
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::Parser};
    fn run(s: &str) -> Result<Program, String> { analyze(&Parser::new(lex(s)?).parse()?) }
    #[test] fn rejects_use_before_declaration() { assert!(run("int main(){ x=2; int x; return 0; }").unwrap_err().contains("undeclared variable 'x'")); }
    #[test] fn rejects_same_scope_redeclaration() { assert!(run("int main(){ int x; int x; return 0; }").unwrap_err().contains("redeclaration")); }
    #[test] fn permits_shadowing() {
        let p=run("int main(){ int x=3; { int x=7; x=x+1; } return x; }").unwrap();
        let Stmt::Declaration{name:outer,..}=&p.statements[0] else { panic!() };
        let Stmt::Block(inner)=&p.statements[1] else { panic!() };
        let Stmt::Declaration{name:inner,..}=&inner[0] else { panic!() };
        assert_ne!(outer,inner);
    }
    #[test] fn resolves_outer_variable() { assert!(run("int main(){ int x=3; { int y=x; } return x; }").is_ok()); }
}