//! Human-readable text form of the IR, used by `--dump-ir`.
//!
//! ```text
//! function gcd(2 params) {
//! bb0:
//!   %0 = param 0
//!   %1 = param 1
//!   jmp bb1
//! bb1:
//!   %2 = phi [bb0: %0], [bb3: %6]
//!   br %4 ? bb3 : bb2          ; non-zero -> bb3, zero -> bb2
//!   ...
//! ```
//!
//! Values are `%N`, basic blocks `bbN`, and locals keep the unique names given by
//! semantic analysis (`x#3`). All integers are 32-bit; arithmetic wraps, and
//! `sdiv`/`srem` are signed.

use std::fmt::Write;
use crate::ir::{Instruction, IrBinary, IrFunction, IrModule, IrUnary, Terminator, ValueId};

/// Optional per-value comment (e.g. the register assigned to it), shown after the
/// instruction that defines the value.
pub type Annotate<'a> = &'a dyn Fn(&IrFunction, ValueId) -> Option<String>;

pub fn module_to_string(module: &IrModule, annotate: Annotate) -> String {
    module.functions.iter().map(|f| function_to_string(f, annotate)).collect::<Vec<_>>().join("\n")
}

pub fn function_to_string(function: &IrFunction, annotate: Annotate) -> String {
    let mut out = String::new();
    let params = match function.param_count { 1 => "1 param".to_string(), n => format!("{n} params") };
    let _ = writeln!(out, "function {}({params}) {{", function.name);
    for block in &function.blocks {
        let entry = if block.id == function.entry { "  ; entry" } else { "" };
        let _ = writeln!(out, "bb{}:{entry}", block.id);
        for instruction in &block.instructions {
            let text = instruction_to_string(instruction);
            let comment = defined_value(instruction).and_then(|v| annotate(function, v));
            match comment {
                Some(c) => { let _ = writeln!(out, "  {text:<40} ; {c}"); }
                None => { let _ = writeln!(out, "  {text}"); }
            }
        }
        let _ = match block.terminator {
            Some(t) => writeln!(out, "  {}", terminator_to_string(t)),
            None => writeln!(out, "  <missing terminator>"),
        };
    }
    out.push_str("}\n");
    out
}

fn defined_value(instruction: &Instruction) -> Option<ValueId> {
    match instruction {
        Instruction::Const { dst, .. } | Instruction::Param { dst, .. } | Instruction::Call { dst, .. }
        | Instruction::LoadLocal { dst, .. } | Instruction::Unary { dst, .. } | Instruction::ToBool { dst, .. }
        | Instruction::Binary { dst, .. } | Instruction::Phi { dst, .. } => Some(*dst),
        Instruction::StoreLocal { .. } => None,
    }
}

pub fn instruction_to_string(instruction: &Instruction) -> String {
    match instruction {
        Instruction::Const { dst, value } => format!("%{dst} = const {value}"),
        Instruction::Param { dst, index } => format!("%{dst} = param {index}"),
        Instruction::Call { dst, callee, args } => {
            let args: Vec<String> = args.iter().map(|a| format!("%{a}")).collect();
            format!("%{dst} = call {callee}({})", args.join(", "))
        }
        Instruction::LoadLocal { dst, name } => format!("%{dst} = load {name}"),
        Instruction::StoreLocal { name, src } => format!("store {name}, %{src}"),
        Instruction::Unary { dst, op, src } => format!("%{dst} = {} %{src}", unary_name(*op)),
        Instruction::ToBool { dst, src } => format!("%{dst} = tobool %{src}"),
        Instruction::Binary { dst, op, lhs, rhs } => format!("%{dst} = {} %{lhs}, %{rhs}", binary_name(*op)),
        Instruction::Phi { dst, incoming } => {
            let arms: Vec<String> = incoming.iter().map(|(block, value)| format!("[bb{block}: %{value}]")).collect();
            format!("%{dst} = phi {}", arms.join(", "))
        }
    }
}

pub fn terminator_to_string(terminator: Terminator) -> String {
    match terminator {
        Terminator::Jump(target) => format!("jmp bb{target}"),
        Terminator::Branch { condition, if_zero, if_nonzero } =>
            format!("br %{condition} ? bb{if_nonzero} : bb{if_zero}"),
        Terminator::Return(value) => format!("ret %{value}"),
    }
}

fn unary_name(op: IrUnary) -> &'static str {
    match op { IrUnary::Plus => "pos", IrUnary::Negate => "neg", IrUnary::BitNot => "not", IrUnary::LogicalNot => "lnot" }
}

fn binary_name(op: IrBinary) -> &'static str {
    match op {
        IrBinary::Add => "add", IrBinary::Sub => "sub", IrBinary::Mul => "mul",
        IrBinary::Div => "sdiv", IrBinary::Rem => "srem",
        IrBinary::BitAnd => "and", IrBinary::BitOr => "or", IrBinary::BitXor => "xor",
        IrBinary::Equal => "eq", IrBinary::NotEqual => "ne", IrBinary::Less => "slt",
        IrBinary::LessEqual => "sle", IrBinary::Greater => "sgt", IrBinary::GreaterEqual => "sge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir, lexer::lex, parser::Parser, sema};

    fn dump(source: &str, optimize: bool) -> String {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        let mut module = ir::lower(&sema::analyze(&ast).unwrap());
        if optimize {
            ir::promote_locals_to_ssa(&mut module);
            module = ir::optimize(module);
        }
        module_to_string(&module, &|_, _| None)
    }

    #[test]
    fn constant_folded_function_prints_exactly() {
        assert_eq!(
            dump("int main(void){return 1+2;}", true),
            "function main(0 params) {\nbb0:  ; entry\n  %2 = const 3\n  ret %2\n}\n"
        );
    }
    #[test]
    fn lowered_form_uses_locals_and_ssa_form_uses_phi() {
        let source = "int main(void){int x=1; if (x < 2) x = x + 3; else x = x + 4; return x;}";
        let lowered = dump(source, false);
        assert!(lowered.contains("= load x#0") && lowered.contains("store x#0,") && !lowered.contains("phi"));
        let ssa = dump("int main(void){int x=0; while (x < 5) x = x + 1; return x;}", true);
        assert!(ssa.contains("= phi [bb0: %") && ssa.contains("br %"), "{ssa}");
        assert!(!ssa.contains("load") && !ssa.contains("store"), "{ssa}");
    }
    #[test]
    fn prints_params_calls_and_all_operator_names() {
        let text = dump("int f(int a,int b){return a/b+a%b-(a*b)+(a&b)+(a|b)+(a^b);} int main(void){return f(7,2)<3;}", true);
        for needle in ["function f(2 params)", "= param 1", "sdiv", "srem", "mul", "and", "or", "xor", "= call f(", "slt"] {
            assert!(text.contains(needle), "missing '{needle}' in\n{text}");
        }
        let unary = dump("int f(int a){return -a + ~a + !a + +a;} int main(void){return f(3);}", true);
        for needle in ["= neg %", "= not %", "= lnot %"] { assert!(unary.contains(needle), "missing '{needle}' in\n{unary}"); }
    }
    #[test]
    fn annotations_are_appended_to_defining_instructions() {
        let ast = Parser::new(lex("int main(void){return 1+2;}").unwrap()).parse().unwrap();
        let module = ir::lower(&sema::analyze(&ast).unwrap());
        let text = module_to_string(&module, &|_, v| Some(format!("value {v}")));
        assert!(text.contains("= const 1") && text.contains("; value 0"), "{text}");
        assert!(!text.lines().any(|l| l.trim_start().starts_with("ret") && l.contains(';')), "{text}");
    }
}
