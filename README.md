# custom-c-compiler

Experimental Rust compiler with a long-term goal of C17/C++20 frontends and a native x86-64 Linux backend. It emits machine code and ELF executables directly, without GCC, Clang, LLVM, an assembler, or a linker.

## Current status: 0.13.0 bootstrap

Implemented subset: multiple `int` functions with up to any number of `int` parameters, prototypes, recursion and mutual recursion; calls following the System V AMD64 calling convention (first six arguments in RDI, RSI, RDX, RCX, R8, R9, the rest on the stack, result in RAX); decimal integer literals; parentheses; unary `+`, `-`, `~`, `!`; arithmetic `+`, `-`, `*`, signed `/`, `%`; bitwise `&`, `|`, `^`; comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`; short-circuit logical `&&`, `||`; local `int` declarations and assignment; nested blocks with lexical scope and shadowing; expression statements and the empty statement; `if`/`else`; `while`; and `return`. Comparisons and logical operators produce integer 0 or 1.

```c
int gcd(int a, int b) {
    while (b) { int t = b; b = a % b; a = t; }
    return a;
}
int main(void) { return gcd(84, 36); }   /* exit status 12 */
```

Semantic analysis resolves names, rejects use before declaration and same-scope redeclarations (a parameter and a top-level local of the same name conflict, as in C), and gives shadowed locals distinct internal names. It also checks functions: calls must target a function declared earlier (use a prototype for forward or mutual recursion), argument counts must match, `main` must exist and take no parameters, and a function that is called must be defined. Definite-assignment analysis rejects reads of uninitialized locals, checks both `if` paths, and conservatively assumes loops may execute zero times. This is not yet a conforming C compiler: `for`, `void`/other types (`int` is 64-bit), full type checking, preprocessing, pointers, arrays, structs, floating point, global variables, and C++ syntax are not implemented. Target is Linux x86-64.

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
- `src/ast.rs`: function, statement and expression AST.
- `src/parser.rs`: top-level functions/prototypes, statements, and a Pratt expression parser (including calls).
- `src/sema.rs`: function signatures and call checking, lexical scopes, identifier resolution, definite-assignment diagnostics.
- `src/ir.rs`: per-function IR (`IrModule`/`IrFunction`) with explicit basic blocks, `Param`/`Call` instructions, virtual values, control-flow terminators, constant folding, constant-branch simplification, unreachable-block pruning, and dead-value elimination.
- `src/regalloc.rs`: block-level liveness and linear-scan allocation to the callee-saved registers RBX/R12-R15 with spill slots.
- `src/codegen.rs`: `_start` stub, per-function prologue/epilogue, System V calls, direct basic-block IR to x86-64 emission, and branch/call fixups.
- `src/elf.rs`: minimal ELF64 executable writer.
- `tests/e2e.rs`: compile-and-run tests (expressions, whole programs with calls, invalid programs).
- `scripts/fuzz_vs_gcc.py`: differential fuzzer comparing `ccc` with `gcc` on random programs (`scripts/fuzz_vs_gcc.py target/release/ccc 500`).

## Milestones

- [x] Direct ELF emission and integer expression backend.
- [x] Local variables, assignment, blocks, `if`/`else`, `while`.
- [x] Comparisons and short-circuit logical operators.
- [x] Separate semantic analysis and lexical scope/shadowing.
- [x] Definite-assignment analysis.
- [x] Target-independent integer IR and constant folding.
- [x] Basic-block IR with explicit control flow and direct native code generation.
- [x] CFG simplification, unreachable-block pruning, and dead-value elimination.
- [x] Explicit phi nodes for short-circuit expression results, with edge-specific copies in the native backend.
- [x] Promote local variables to SSA with dominator trees, dominance frontiers, and phi nodes at control-flow joins.
- [x] Multiple functions, prototypes, recursion, and System V AMD64 calls (register and stack arguments).
- [ ] C types (including a real 32-bit `int`), pointers, arrays, structs, enums, preprocessing and multiple translation units.
- [ ] ELF relocatable objects, symbols, relocations and linking.
- [ ] Broader C17 conformance, then incremental C++20 features.
- [x] Liveness intervals and linear-scan allocation to x86-64 registers with spill slots.
- [ ] More advanced register allocation, debug info and additional targets.

The optimizer folds constants, simplifies branches whose conditions are known, removes unreachable blocks, and iteratively deletes unused pure computations while preserving stores and calls. The IR has explicit SSA-style phi nodes for short-circuit expression results; local variables (including parameters) are promoted to SSA with phi nodes at control-flow joins. The backend assigns virtual values to RBX/R12-R15 using real block-level liveness (so values stay allocated across loop back-edges) and spills excess live ranges to stack slots; because those registers are callee-saved, values in them survive calls. More advanced allocation remains future work. Full C17/C++20 compatibility is a long-term goal, not a current capability.
