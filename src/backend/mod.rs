//! Code generation. Everything specific to a CPU architecture lives in one
//! submodule that implements [`Backend`]; this file is the architecture-independent
//! part: choosing a backend for a [`Target`] and linking compiled functions
//! into one block of machine code.
//!
//! # Adding an architecture
//! 1. Add a variant to [`crate::target::Arch`] (the compiler will then list every
//!    `match` that needs a new arm).
//! 2. Create `backend/<arch>/` with its register set and ABI, an instruction
//!    encoder, and a type implementing [`Backend`]. `x86_64` is the model.
//! 3. Return it from [`for_target`].
//! 4. If the architecture needs a different entry sequence or system call numbers
//!    for an operating system, handle that in `Backend::entry_stub`.
//!
//! Adding an operating system is mostly the executable file format (see
//! [`crate::object`]) plus its entry stub and calling-convention differences.

pub mod x86_64;

use std::collections::HashMap;
use crate::ir::{IrFunction, IrModule};
use crate::regalloc::PhysReg;
use crate::target::{Arch, Os, Target};

/// A `call` whose destination is not known until all functions are laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    /// Offset of the bytes to patch (e.g. the `rel32` of an x86-64 `call`).
    pub offset: usize,
    pub callee: String,
}

/// Machine code for one function (or the entry stub) and its unresolved calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCode {
    pub code: Vec<u8>,
    pub calls: Vec<CallSite>,
}

pub trait Backend {
    fn name(&self) -> &'static str;
    /// Registers the allocator may give to long-lived values, in preference order.
    fn allocatable_registers(&self) -> &'static [PhysReg];
    fn register_name(&self, reg: PhysReg) -> &'static str;
    /// Translate one function. Calls are recorded, not resolved.
    fn compile_function(&self, function: &IrFunction) -> Result<FunctionCode, String>;
    /// The program's entry point: call `main`, then exit the process with its
    /// result using `os`'s mechanism. Its `calls` must contain the call to `main`.
    fn entry_stub(&self, os: Os) -> Result<FunctionCode, String>;
    /// Make the call at `site` (an offset into `code`) transfer to `target`
    /// (another offset into `code`).
    fn patch_call(&self, code: &mut [u8], site: usize, target: usize) -> Result<(), String>;
}

pub fn for_target(target: &Target) -> Box<dyn Backend> {
    match target.arch { Arch::X86_64 => Box::new(x86_64::X86_64) }
}

/// Compile the whole module: the entry stub first, then every function in
/// order, with all calls resolved. The result starts at the program entry point.
pub fn generate(backend: &dyn Backend, os: Os, module: &IrModule) -> Result<Vec<u8>, String> {
    let entry = backend.entry_stub(os)?;
    let mut code = entry.code;
    let mut calls: Vec<CallSite> = entry.calls;
    let mut offsets = HashMap::<String, usize>::new();
    for function in &module.functions {
        if offsets.insert(function.name.clone(), code.len()).is_some() {
            return Err(format!("invalid IR: duplicate function '{}'", function.name));
        }
        let compiled = backend.compile_function(function)?;
        let base = code.len();
        code.extend_from_slice(&compiled.code);
        calls.extend(compiled.calls.into_iter().map(|c| CallSite { offset: base + c.offset, callee: c.callee }));
    }
    for CallSite { offset, callee } in calls {
        let target = *offsets.get(&callee).ok_or_else(|| format!("undefined function '{callee}'"))?;
        backend.patch_call(&mut code, offset, target)?;
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir, lexer::lex, parser::Parser, sema};

    /// A toy architecture: every function is 8 bytes `[0xAA; 8]` whose first four
    /// bytes are a call-site placeholder, and "patching" stores the little-endian
    /// absolute target. It shares nothing with x86-64, which is the point: the
    /// linking driver must work for any `Backend`.
    struct Toy;
    impl Backend for Toy {
        fn name(&self) -> &'static str { "toy" }
        fn allocatable_registers(&self) -> &'static [PhysReg] { &[1, 2] }
        fn register_name(&self, _: PhysReg) -> &'static str { "r?" }
        fn compile_function(&self, function: &IrFunction) -> Result<FunctionCode, String> {
            // Calls the next function by name if the IR contains a call.
            let callee = function.blocks.iter().flat_map(|b| &b.instructions).find_map(|i| match i {
                ir::Instruction::Call { callee, .. } => Some(callee.clone()), _ => None });
            Ok(FunctionCode { code: vec![0xAA; 8], calls: callee.map(|c| CallSite { offset: 0, callee: c }).into_iter().collect() })
        }
        fn entry_stub(&self, _: Os) -> Result<FunctionCode, String> {
            Ok(FunctionCode { code: vec![0xEE; 4], calls: vec![CallSite { offset: 0, callee: "main".into() }] })
        }
        fn patch_call(&self, code: &mut [u8], site: usize, target: usize) -> Result<(), String> {
            code[site..site + 4].copy_from_slice(&(target as u32).to_le_bytes());
            Ok(())
        }
    }

    fn module(source: &str) -> IrModule {
        let ast = Parser::new(lex(source).unwrap()).parse().unwrap();
        ir::lower(&sema::analyze(&ast).unwrap())
    }

    #[test] fn links_functions_in_order_after_the_entry_stub() {
        let m = module("int helper(void){return 1;} int main(void){return helper();}");
        let code = generate(&Toy, Os::Linux, &m).unwrap();
        // stub (4 bytes) + helper (8) + main (8)
        assert_eq!(code.len(), 4 + 8 + 8);
        // The stub calls main at offset 12; main calls helper at offset 4.
        assert_eq!(&code[0..4], 12u32.to_le_bytes());
        assert_eq!(&code[12..16], 4u32.to_le_bytes());
        assert_eq!(&code[4..8], [0xAA; 4]); // helper has no call, so it keeps its bytes
    }
    #[test] fn reports_undefined_and_duplicate_functions() {
        let mut m = module("int main(void){return 0;}");
        m.functions[0].name = "other".into();
        assert!(generate(&Toy, Os::Linux, &m).unwrap_err().contains("undefined function 'main'"));
        let mut m = module("int a(void){return 0;} int main(void){return 0;}");
        m.functions[0].name = "main".into();
        assert!(generate(&Toy, Os::Linux, &m).unwrap_err().contains("duplicate function 'main'"));
    }
    #[test] fn selects_the_backend_for_a_target() {
        let backend = for_target(&Target::DEFAULT);
        assert_eq!(backend.name(), "x86_64");
        assert!(!backend.allocatable_registers().is_empty());
        assert_eq!(backend.register_name(backend.allocatable_registers()[0]), "rbx");
    }
    #[test] fn x86_64_entry_stub_calls_main_then_exits() {
        let stub = for_target(&Target::DEFAULT).entry_stub(Os::Linux).unwrap();
        assert_eq!(stub.calls, vec![CallSite { offset: 1, callee: "main".into() }]);
        assert_eq!(stub.code, [0xe8, 0, 0, 0, 0, 0x48, 0x89, 0xc7, 0xb8, 60, 0, 0, 0, 0x0f, 0x05]);
    }
}
