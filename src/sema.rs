use std::collections::{HashMap, HashSet};
use crate::ast::{Expr, Function, Program, Stmt, UnaryOp};

/// Resolve identifiers and validate declarations while preserving lexical scope,
/// check function declarations and calls, then reject reads of locals that are
/// not definitely initialized.
pub fn analyze(program: &Program) -> Result<Program, String> {
    let mut analyzer = Analyzer {
        scopes: Vec::new(), next_id: 0, signatures: HashMap::new(), called: HashSet::new(),
    };
    let mut defined = HashSet::<String>::new();
    let mut functions = Vec::new();
    for f in &program.functions {
        // Register the signature first so a function may call itself, but only
        // functions declared *earlier* are visible (as in C).
        match analyzer.signatures.get(&f.name) {
            Some(arity) if *arity != f.params.len() =>
                return Err(format!("conflicting declaration of function '{}'", f.name)),
            _ => {}
        }
        analyzer.signatures.insert(f.name.clone(), f.params.len());
        if f.body.is_some() && !defined.insert(f.name.clone()) {
            return Err(format!("redefinition of function '{}'", f.name));
        }
        functions.push(analyzer.function(f)?);
    }
    match analyzer.signatures.get("main") {
        Some(0) if defined.contains("main") => {}
        Some(0) => return Err("function 'main' is declared but never defined".into()),
        Some(_) => return Err("'main' must take no parameters".into()),
        None => return Err("program has no 'main' function".into()),
    }
    let mut missing: Vec<&String> = analyzer.called.iter().filter(|n| !defined.contains(*n)).collect();
    missing.sort();
    if let Some(name) = missing.first() {
        return Err(format!("undefined function '{name}' (declared but never defined)"));
    }
    Ok(Program { functions })
}

struct Analyzer {
    scopes: Vec<HashMap<String, String>>,
    next_id: usize,
    signatures: HashMap<String, usize>,
    called: HashSet<String>,
}
impl Analyzer {
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self) { self.scopes.pop(); }
    fn unique(&mut self, name: &str) -> String {
        let unique = format!("{name}#{}", self.next_id); self.next_id += 1; unique
    }
    fn resolve(&self, name: &str) -> Result<String, String> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
            .ok_or_else(|| format!("use of undeclared variable '{name}'"))
    }
    /// Parameters live in the same scope as the function body's outermost block,
    /// so `int f(int a) { int a; }` is a redeclaration error, as in C.
    fn function(&mut self, f: &Function) -> Result<Function, String> {
        self.scopes = vec![HashMap::new()];
        let mut params = Vec::new();
        for p in &f.params {
            if self.scopes[0].contains_key(p) {
                return Err(format!("duplicate parameter '{p}' in function '{}'", f.name));
            }
            let unique = self.unique(p);
            self.scopes[0].insert(p.clone(), unique.clone());
            params.push(unique);
        }
        let body = match &f.body {
            None => None,
            Some(body) => {
                let resolved = self.statements(body)?;
                let mut flow = Flow { initialized: params.iter().cloned().collect() };
                flow.statements(&resolved)?;
                Some(resolved)
            }
        };
        self.scopes.clear();
        Ok(Function { name: f.name.clone(), params, body })
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
                let unique = self.unique(name);
                self.scopes.last_mut().unwrap().insert(name.clone(), unique.clone());
                Ok(Stmt::Declaration { name: unique, initializer })
            }
            Stmt::Assignment { name, value } => {
                let name = self.resolve(name)?; let value = self.expr(value)?;
                Ok(Stmt::Assignment { name, value })
            }
            Stmt::Return(e) => Ok(Stmt::Return(self.expr(e)?)),
            Stmt::Expr(e) => Ok(Stmt::Expr(self.expr(e)?)),
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
    fn expr(&mut self, e: &Expr) -> Result<Expr, String> {
        Ok(match e {
            // `-2147483648` is the one literal that only fits once the minus is applied.
            Expr::Unary { op: UnaryOp::Minus, expr } if matches!(**expr, Expr::Number(n) if n == 1 << 31) => {
                Expr::Number(i64::from(i32::MIN))
            }
            Expr::Number(n) => {
                if i32::try_from(*n).is_err() {
                    return Err(format!("integer literal {n} is too large for 'int' (the maximum is 2147483647)"));
                }
                Expr::Number(*n)
            }
            Expr::Variable(n) => Expr::Variable(self.resolve(n)?),
            Expr::Unary { op, expr } => Expr::Unary { op: *op, expr: Box::new(self.expr(expr)?) },
            Expr::Binary { op, left, right } => Expr::Binary {
                op: *op, left: Box::new(self.expr(left)?), right: Box::new(self.expr(right)?)
            },
            Expr::Call { name, args } => {
                if self.scopes.iter().any(|s| s.contains_key(name)) {
                    return Err(format!("called object '{name}' is a variable, not a function"));
                }
                let arity = *self.signatures.get(name)
                    .ok_or_else(|| format!("call to undeclared function '{name}'"))?;
                if arity != args.len() {
                    return Err(format!("function '{name}' expects {arity} argument(s), got {}", args.len()));
                }
                self.called.insert(name.clone());
                let args = args.iter().map(|a| self.expr(a)).collect::<Result<Vec<_>, _>>()?;
                Expr::Call { name: name.clone(), args }
            }
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
            Stmt::Return(e) | Stmt::Expr(e) => self.expr(e)?,
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
            Expr::Call { args, .. } => { for a in args { self.expr(a)?; } Ok(()) }
            Expr::Binary { left, right, .. } => {
                self.expr(left)?;
                // This subset has no assignment expressions, so checking both operands
                // is conservative and catches any potentially evaluated uninitialized read.
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
    fn main_body(p: &Program) -> &Vec<Stmt> { p.functions.iter().find(|f| f.name == "main").unwrap().body.as_ref().unwrap() }

    #[test] fn rejects_use_before_declaration() {
        assert!(run("int main(){ x=2; int x; return 0; }").unwrap_err().contains("undeclared variable 'x'"));
    }
    #[test] fn rejects_same_scope_redeclaration() {
        assert!(run("int main(){ int x; int x; return 0; }").unwrap_err().contains("redeclaration"));
    }
    #[test] fn permits_shadowing() {
        let p=run("int main(){ int x=3; { int x=7; x=x+1; } return x; }").unwrap();
        let body=main_body(&p);
        let Stmt::Declaration{name:outer,..}=&body[0] else { panic!() };
        let Stmt::Block(inner)=&body[1] else { panic!() };
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

    #[test] fn params_are_initialized_locals() {
        assert!(run("int f(int a,int b){ return a+b; } int main(){ return f(1,2); }").is_ok());
    }
    #[test] fn params_get_unique_names_per_function() {
        let p=run("int f(int a){return a;} int g(int a){return a;} int main(){return f(1)+g(2);}").unwrap();
        assert_ne!(p.functions[0].params[0], p.functions[1].params[0]);
    }
    #[test] fn rejects_param_redeclared_in_body() {
        assert!(run("int f(int a){ int a; return 0; } int main(){return 0;}").unwrap_err().contains("redeclaration"));
    }
    #[test] fn permits_param_shadowed_in_inner_block() {
        assert!(run("int f(int a){ { int a=2; return a; } } int main(){return f(1);}").is_ok());
    }
    #[test] fn rejects_duplicate_params() {
        assert!(run("int f(int a,int a){return a;} int main(){return 0;}").unwrap_err().contains("duplicate parameter"));
    }
    #[test] fn requires_main() {
        assert!(run("int f(void){return 1;}").unwrap_err().contains("no 'main'"));
        assert!(run("int main(int argc){return 0;}").unwrap_err().contains("no parameters"));
    }
    #[test] fn rejects_undeclared_function_and_use_before_declaration() {
        assert!(run("int main(){return f();}").unwrap_err().contains("undeclared function 'f'"));
        assert!(run("int main(){return f();} int f(void){return 1;}").is_err());
    }
    #[test] fn prototypes_allow_forward_and_mutual_calls() {
        assert!(run("int odd(int n); int even(int n){ if(n==0) return 1; return odd(n-1);} int odd(int n){ if(n==0) return 0; return even(n-1);} int main(){return even(4);}").is_ok());
    }
    #[test] fn rejects_wrong_arity() {
        assert!(run("int f(int a){return a;} int main(){return f(1,2);}").unwrap_err().contains("expects 1"));
        assert!(run("int f(int a){return a;} int main(){return f();}").is_err());
    }
    #[test] fn rejects_conflicting_and_duplicate_definitions() {
        assert!(run("int f(int a); int f(int a,int b){return 0;} int main(){return 0;}").unwrap_err().contains("conflicting"));
        assert!(run("int f(void){return 1;} int f(void){return 2;} int main(){return 0;}").unwrap_err().contains("redefinition"));
    }
    #[test] fn rejects_call_of_declared_but_undefined_function() {
        assert!(run("int f(void); int main(){return f();}").unwrap_err().contains("never defined"));
    }
    #[test] fn rejects_calling_a_variable() {
        assert!(run("int f(void){return 1;} int main(){int f=1; return f();}").unwrap_err().contains("not a function"));
    }
    #[test] fn call_arguments_must_be_initialized() {
        assert!(run("int f(int a){return a;} int main(){int x; return f(x);}").unwrap_err().contains("uninitialized"));
    }

    #[test] fn int_literal_range() {
        assert!(run("int main(){return 2147483647;}").is_ok());
        assert!(run("int main(){return 2147483648;}").unwrap_err().contains("too large for 'int'"));
        assert!(run("int main(){return 4294967296 == 0;}").is_err());
    }
    #[test] fn int_min_literal_is_allowed_under_minus() {
        let p = run("int main(){return -2147483648;}").unwrap();
        assert_eq!(main_body(&p)[0], Stmt::Return(Expr::Number(i64::from(i32::MIN))));
        assert!(run("int main(){return -2147483649;}").is_err());
    }
}
