# custom-c-compiler

Experimental C17/C++20 compiler implemented in Rust. The project aims to share frontend infrastructure and use a fully native x86-64 backend that emits Linux ELF binaries without invoking GCC, Clang, an assembler, or a linker for the generated program.

## Stage 1

A first vertical slice accepts a tiny C subset:

    int main(void) { return 42; }

It writes a minimal static x86-64 Linux ELF executable directly. The generated program exits through the Linux exit syscall. This is a bootstrap milestone, not full C17 support.

## Build

Requires Rust stable; generated executables run on Linux x86-64.

    cargo build --release
    printf 'int main(void) { return 42; }\n' > test.c
    ./target/release/ccc -o test test.c
    ./test
    echo $?

Expected exit status: 42.

## Architecture

- Lexer and parser: tokens, source spans, C/C++ grammar modes.
- Semantic analysis: declarations, scopes, name lookup, type checking.
- Shared typed IR: language-neutral representation, control-flow graph, later SSA.
- Optimizer: constant folding, dead-code elimination, inlining.
- x86-64 backend: instruction selection, register allocation, ABI lowering.
- ELF writer: executable first, then relocatable objects and relocations.
- Driver: CLI, diagnostics, include paths and target options.

## Milestones

1. Bootstrap parser and direct ELF emission.
2. Expressions, precedence and arithmetic machine code.
3. Locals, branches, loops, stack frames and function calls.
4. C frontend: types, pointers, arrays, structs, preprocessing and translation units.
5. ELF objects, relocations and external linking.
6. C17 conformance suite.
7. C++20 features: classes, references, overload resolution, templates, exceptions and standard library compatibility.
8. Optimization, debug information, AArch64 and Windows targets.

No LLVM dependency is planned in the code-generation path. Full C17/C++20 compatibility is a long-term goal, not a current claim.
