# custom-c-compiler

Experimental Rust compiler with a long-term goal of C17/C++20 frontends and a native x86-64 Linux backend. It emits machine code and ELF executables directly, without GCC, Clang, LLVM, an assembler, or a linker.

## Current status: 0.14.0 bootstrap

Implemented subset: `int` is a real 32-bit signed type (arithmetic wraps modulo 2^32 like `gcc -fwrapv`; integer literals above 2147483647 are rejected, while `-2147483648` is accepted); multiple `int` functions with up to any number of `int` parameters, prototypes, recursion and mutual recursion; calls following the System V AMD64 calling convention (first six arguments in RDI, RSI, RDX, RCX, R8, R9, the rest on the stack, result in RAX); decimal integer literals; parentheses; unary `+`, `-`, `~`, `!`; arithmetic `+`, `-`, `*`, signed `/`, `%`; bitwise `&`, `|`, `^`; comparisons `==`, `!=`, `<`, `<=`, `>`, `>=`; short-circuit logical `&&`, `||`; local `int` declarations and assignment; nested blocks with lexical scope and shadowing; expression statements and the empty statement; `if`/`else`; `while`; and `return`. Comparisons and logical operators produce integer 0 or 1.

```c
int gcd(int a, int b) {
    while (b) { int t = b; b = a % b; a = t; }
    return a;
}
int main(void) { return gcd(84, 36); }   /* exit status 12 */
```

Semantic analysis resolves names, rejects use before declaration and same-scope redeclarations (a parameter and a top-level local of the same name conflict, as in C), and gives shadowed locals distinct internal names. It also checks functions: calls must target a function declared earlier (use a prototype for forward or mutual recursion), argument counts must match, `main` must exist and take no parameters, and a function that is called must be defined. Definite-assignment analysis rejects reads of uninitialized locals, checks both `if` paths, and conservatively assumes loops may execute zero times. This is not yet a conforming C compiler: `for`, `void` and other types such as `long`/`char`/`unsigned`, full type checking, preprocessing, pointers, arrays, structs, floating point, global variables, and C++ syntax are not implemented. Target is Linux x86-64.

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

### Inspecting what the compiler does

```sh
ccc --dump-ast f.c            # the parsed AST
ccc --dump-ir=lowered f.c     # IR straight from the AST (locals live in memory)
ccc --dump-ir=ssa f.c         # after promoting locals to SSA (phi nodes appear)
ccc --dump-ir f.c             # after optimization (same as --dump-ir=opt)
ccc --dump-regalloc f.c       # optimized IR + register/spill slot and live range per value
ccc --dump-ir -o f f.c        # dump and still produce the executable
```

Dumps go to stdout. Without `-o` nothing is written to disk. Example (`--dump-regalloc` on a `gcd` function):

```text
function gcd(2 params) {
bb0:  ; entry
  %0 = param 0                             ; rbx  live [0, 5]
  %1 = param 1                             ; r12  live [2, 5]
  jmp bb1
bb1:
  %10 = phi [bb0: %0], [bb2: %9]           ; rbx  live [6, 16]
  %9 = phi [bb0: %1], [bb2: %6]            ; r12  live [8, 15]
  br %9 ? bb2 : bb3
bb2:
  %6 = srem %10, %9                        ; r13  live [12, 15]
  jmp bb1
bb3:
  ret %10
}
```

`br %c ? bbA : bbB` means "if `%c` is non-zero go to `bbA`, otherwise `bbB`". `sdiv`/`srem`/`slt`/... are signed 32-bit operations.

### Targets

```sh
ccc --target=x86_64-linux -o f f.c     # the default, and currently the only target
ccc --target=aarch64-linux -o f f.c    # ccc: error: unsupported architecture 'aarch64' ... (supported targets: x86_64-linux)
```

A target is an architecture plus an operating system. See `docs/architecture.md` ("Targets and backends") for what adding one involves.

### Differential fuzzing against gcc

```sh
scripts/fuzz_vs_gcc.py target/release/ccc 500      # needs gcc; keeps failing programs in /tmp/fz
```

## Source layout

- `src/lexer.rs`: tokens, offsets, C comments.
- `src/ast.rs`: function, statement and expression AST.
- `src/parser.rs`: top-level functions/prototypes, statements, and a Pratt expression parser (including calls).
- `src/sema.rs`: function signatures and call checking, lexical scopes, identifier resolution, definite-assignment diagnostics.
- `src/irdump.rs`: text form of the IR for `--dump-ir` / `--dump-regalloc`.
- `src/ir.rs`: per-function IR (`IrModule`/`IrFunction`) with explicit basic blocks, `Param`/`Call` instructions, virtual values, control-flow terminators, constant folding, constant-branch simplification, unreachable-block pruning, and dead-value elimination.
- `src/regalloc.rs`: block-level liveness and target-independent linear-scan allocation over whatever registers the backend offers, with spill slots.
- `src/target.rs`: `Arch`, `Os` and `Target` (`--target=ARCH-OS`).
- `src/backend/mod.rs`: the `Backend` trait (one implementation per architecture), backend selection, and the architecture-independent step that links compiled functions and resolves calls.
- `src/backend/x86_64/`: the x86-64 backend. `abi.rs` has the registers and System V facts (checked at build time), `encode.rs` is the only file with opcode bytes, `mod.rs` does frame layout, instruction selection, calls and phi copies.
- `src/object/`: executable file formats chosen by operating system; `elf.rs` writes ELF64.
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
- [x] 32-bit `int` semantics (wrapping arithmetic, literal range checks), `--dump-ast` / `--dump-ir` / `--dump-regalloc`.
- [ ] More C types (`char`, `long`, `unsigned`, pointers), arrays, structs, enums, preprocessing and multiple translation units.
- [ ] ELF relocatable objects, symbols, relocations and linking.
- [ ] Broader C17 conformance, then incremental C++20 features.
- [x] Liveness intervals and linear-scan allocation to x86-64 registers with spill slots.
- [ ] More advanced register allocation, debug info and additional targets.

The optimizer folds constants, simplifies branches whose conditions are known, removes unreachable blocks, and iteratively deletes unused pure computations while preserving stores and calls. The IR has explicit SSA-style phi nodes for short-circuit expression results; local variables (including parameters) are promoted to SSA with phi nodes at control-flow joins. The backend assigns virtual values to RBX/R12-R15 using real block-level liveness (so values stay allocated across loop back-edges) and spills excess live ranges to stack slots; because those registers are callee-saved, values in them survive calls. More advanced allocation remains future work. Full C17/C++20 compatibility is a long-term goal, not a current capability.
