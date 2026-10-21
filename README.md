# custom-c-compiler

An experimental compiler written in Rust, targeting C17 and C++20 with a shared frontend/IR architecture and a native x86-64 Linux backend. The backend emits machine code and serializes ELF executables directly; the generated program is not passed through GCC, Clang, LLVM, an assembler, or a linker.

## Current status: 0.2.0 bootstrap

The prototype currently accepts a narrow subset of C:

```c
int main(void) {
    return 2 + 3 * 4;
}
```

Supported expressions: decimal integer literals, parentheses, unary `+`, unary `-`, bitwise complement `~`, binary `+`, `-`, `*`, signed `/`, signed `%`, bitwise `&`, `|`, and `^`. Operator precedence is parsed with a Pratt parser. Only a single `int main(...)` function containing one return statement is accepted. Variables, declarations, preprocessing, strings, floating point, and C++ syntax are not implemented yet.

## Build and run

Requires stable Rust. Generated binaries target Linux x86-64.

```sh
cargo test --all-targets
cargo build --release
printf 'int main(void) { return 2 + 3 * 4; }\n' > test.c
./target/release/ccc -o test test.c
./test
echo $?
```

The program exits with status 14. This is a native Linux process exit status, not text printed to stdout.

## Source layout

- `src/lexer.rs`: tokens and source offsets; skips whitespace and C comments.
- `src/parser.rs`: bootstrap grammar, AST expression types and precedence parsing.
- `src/codegen.rs`: x86-64 instruction emission for integer expressions.
- `src/elf.rs`: minimal ELF64 executable serialization.
- `src/main.rs`: command-line driver and pipeline orchestration.
- `tests/e2e.rs`: compiles a C input and runs the resulting ELF binary.

## Architecture direction

The current files form a small vertical slice. The intended architecture separates:

1. **Driver and diagnostics** — CLI, source files, options and source locations.
2. **Frontend** — reusable lexer, separate C and C++ grammar modes, AST and semantic analysis.
3. **Typed IR** — target-independent typed expressions, statements and control-flow graph.
4. **Optimizer** — constant folding, dead-code elimination, later SSA-based passes.
5. **Target backend** — x86-64 instruction selection, register allocation, stack frames and ABI lowering.
6. **Object writer** — ELF relocatable objects, symbols and relocations; eventually external linking.
7. **Conformance suites** — language feature tests, negative tests and differential tests against reference compilers.

## Milestones

- [x] Stage 1: minimal parser and direct ELF executable emission.
- [x] Stage 2 bootstrap: integer expressions, operator precedence, unary operators and arithmetic/bitwise machine code.
- [ ] Stage 3: local variables, assignment, blocks, conditionals, loops and stack frames.
- [ ] Stage 4: proper C types, pointers, arrays, structs, enums, preprocessing and multiple translation units.
- [ ] Stage 5: ELF object files, relocations, symbols and linking against system libraries.
- [ ] Stage 6: broader C17 compatibility and conformance testing.
- [ ] Stage 7: C++20 frontend features, incrementally including classes, references, overloads, templates and exceptions.
- [ ] Stage 8: optimization, debug info, AArch64 and other object formats/targets.

Full C17/C++20 compatibility is a long-term goal, not a claim about the current prototype.
