# custom-c-compiler

Experimental Rust compiler with a long-term goal of C17/C++20 frontends and a native x86-64 Linux backend. It emits machine code and ELF executables directly, without GCC, Clang, LLVM, an assembler, or a linker.

## Current status: 0.7.0 bootstrap

Implemented subset: one `int main()` or `int main(void)` function; decimal integer literals; parentheses; unary `+`, `-`, `~`, `!`; arithmetic `+`, `-`, `*`, signed `/`, `%`; bitwise `&`, `|`, `^`; comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`; short-circuit logical `&&`, `||`; local `int` declarations and assignment; nested blocks with lexical scope and shadowing; `if`/`else`; `while`; and `return`. Comparisons and logical operators produce integer 0 or 1.

```c
int main(void) {
    int x = 0;
    while (x < 5) x = x + 1;
    if (x == 5 && !(x < 0)) return 42;
    return 0;
}
```

Semantic analysis resolves names, rejects use before declaration and same-scope redeclarations, and gives shadowed locals distinct internal names. Definite-assignment analysis rejects reads of uninitialized locals, checks both `if` paths, and conservatively assumes loops may execute zero times. An assignment in a loop therefore does not by itself initialize a variable after the loop. The analysis does not yet model unreachable code or all C control-flow rules. This is not yet a conforming C compiler: `for`, function calls, multiple functions, full type checking, preprocessing, pointers, arrays, structs, floating point, and C++ syntax are not implemented. Target is Linux x86-64.

## Build and run

```sh
cargo test --all-targets
cargo build --release
printf 'int main(void) { int x=0; while (x<5) x=x+1; return x; }\\n' > test.c
./target/release/ccc -o test test.c
./test
echo $?
```

Expected exit status: `5`.

## Source layout

- `src/lexer.rs`: tokens, offsets, C comments.
- `src/ast.rs`: statement and expression AST.
- `src/parser.rs`: grammar and Pratt expression parser.
- `src/sema.rs`: lexical scopes, identifier resolution, definite-assignment diagnostics.
- `src/ir.rs`: target-independent integer IR, lowering and constant folding.
- `src/codegen.rs`: stack slots, x86-64 emission, branch fixups, short-circuit control flow.
- `src/elf.rs`: minimal ELF64 executable writer.
- `tests/e2e.rs`: compile-and-run tests.

## Milestones

- [x] Direct ELF emission and integer expression backend.
- [x] Local variables, assignment, blocks, `if`/`else`, `while`.
- [x] Comparisons and short-circuit logical operators.
- [x] Separate semantic analysis and lexical scope/shadowing.
- [x] Definite-assignment analysis.
- [x] Target-independent integer IR and constant folding.
- [ ] Direct IR-to-machine-code lowering and basic-block IR.
- [ ] C types, pointers, arrays, structs, enums, preprocessing and multiple translation units.
- [ ] ELF relocatable objects, symbols, relocations and linking.
- [ ] Broader C17 conformance, then incremental C++20 features.
- [ ] Register allocation, optimization, debug info and additional targets.

Full C17/C++20 compatibility is a long-term goal, not a current capability.
