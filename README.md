# custom-c-compiler

Experimental Rust compiler with a long-term goal of C17/C++20 frontends and a native x86-64 Linux backend. It emits machine code and ELF executables directly, without GCC, Clang, LLVM, an assembler, or a linker.

## Current status: 0.4.0 bootstrap

Implemented subset: one `int main()` or `int main(void)` function; decimal integer literals; parentheses; unary `+`, `-`, `~`, `!`; arithmetic `+`, `-`, `*`, signed `/`, `%`; bitwise `&`, `|`, `^`; comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`; logical `&&`, `||`; local `int` declarations and assignment; nested blocks; `if`/`else`; `while`; and `return`. Comparisons and logical operators produce integer 0 or 1.

```c
int main(void) {
    int x = 0;
    while (x < 5) x = x + 1;
    if (x == 5 && !(x < 0)) return 42;
    return 0;
}
```

This is not yet a conforming C compiler. Logical operators currently evaluate both operands and therefore do not implement C's short-circuit side-effect semantics. Lexical scope/shadowing, `for`, function calls, multiple functions, type checking, preprocessing, pointers, arrays, structs, floating point, and C++ syntax are not implemented. Conditions use zero=false and nonzero=true. Target is Linux x86-64.

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
- `src/codegen.rs`: stack slots, x86-64 emission, branch fixups.
- `src/elf.rs`: minimal ELF64 executable writer.
- `tests/e2e.rs`: compile-and-run tests.

## Milestones

- [x] Direct ELF emission and integer expression backend.
- [x] Local variables, assignment, blocks, `if`/`else`, `while`.
- [x] Comparisons and logical operators.
- [ ] Proper lexical scope and separate semantic analysis.
- [ ] Target-independent typed IR and constant folding.
- [ ] C types, pointers, arrays, structs, enums, preprocessing and multiple translation units.
- [ ] ELF relocatable objects, symbols, relocations and linking.
- [ ] Broader C17 conformance, then incremental C++20 features.
- [ ] Register allocation, optimization, debug info and additional targets.

Full C17/C++20 compatibility is a long-term goal, not a current capability.
