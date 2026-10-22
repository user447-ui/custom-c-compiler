# Compiler architecture

```text
CLI -> lexer -> parser/AST (functions) -> semantic analysis -> per-function basic-block IR -> constant folding -> CFG simplification -> dead-value elimination -> phi lowering -> direct native x86-64 backend (`_start` stub + functions) -> ELF64 executable
```

The frontend and semantic checks operate on the resolved AST. `src/ir.rs` lowers the AST into a target-independent control-flow graph. Each basic block contains instructions over virtual values and ends in an explicit jump, conditional branch, or return. Logical `&&` and `||` are lowered to branches so the right-hand operand executes only when required; their results are joined by explicit phi nodes. The native backend routes incoming edges through tiny copy trampolines, ensuring only the selected predecessor value is copied into the phi result slot. Constant folding runs within individual blocks and uses checked arithmetic, so overflow, division by zero, and signed division overflow are not folded. Follow-up passes simplify branches with known conditions, remove unreachable blocks and remap their edges, then iteratively remove unused pure computations while preserving stores and terminators.

The native backend consumes this IR directly; the former IR-to-AST adapter has been removed. It assigns stack homes to remaining locals and spills, allocates virtual values to RBX and R12-R15 with linear scan, emits each block in order, and patches relative branch displacements after block offsets are known. Stack homes keep ordinary local variables straightforward for this bootstrap. Local variables are promoted to SSA; phi copies are staged through temporary stack slots to preserve parallel-copy semantics. The generated process starts at a small `_start` stub that calls `main` and exits through Linux x86-64 syscall 60 with its result. Output is a minimal ELF64 executable, not a relocatable object file.

The bootstrap grammar accepts a sequence of `int` function definitions and prototypes (`int f(int a, int b) { ... }`, `int f(void);`). Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and short-circuit logical operators.

The semantic pass maintains lexical scopes, resolves uses to the nearest visible declaration, rejects undeclared uses and same-scope redeclarations, and rewrites declarations to unique internal names. Definite-assignment analysis tracks initialized locals, intersects branch outputs, and assumes loops may execute zero times. The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. RAX and RCX are scratch registers for operations, while allocated values remain in physical registers or spill slots. 

Local-variable SSA construction uses iterative dominator sets, immediate dominators, dominance frontiers, phi placement, and variable renaming. Phi operands are filled from predecessor exit versions, which handles loop backedges.

## Functions and the calling convention

Each function with a body is lowered to its own `IrFunction`. Parameters are ordinary locals initialised at entry by `Param { index }` instructions, so SSA promotion treats them like any other variable. `Call { callee, args }` has side effects and is never removed as dead code.

The backend follows the System V AMD64 ABI. The first six integer arguments travel in RDI, RSI, RDX, RCX, R8, R9; further arguments are pushed right-to-left, and the caller removes them after the call. RSP is 16-byte aligned at every `call`: the frame size is a multiple of 16, and an odd number of stack arguments is preceded by an 8-byte pad. The result is returned in RAX.

Each function has the frame layout (8-byte slots addressed from RBP): locals, register spill slots, phi staging slots, the incoming register arguments, and the callee-saved registers the function uses. The prologue stores the incoming argument registers immediately, because RCX and RDX are scratch registers in the generated code, and saves the callee-saved registers it uses; every return restores them and executes `mov rsp, rbp; pop rbp; ret`. The allocatable registers RBX and R12-R15 are all callee-saved, so a value kept in one of them survives a call without any caller-side spilling.

Call targets are resolved after all functions are emitted: each `call rel32` is recorded with the callee name and patched once function offsets are known.

## Integer model

C `int` is a 32-bit signed integer. The IR has exactly one value type with those semantics: `Const` holds an `i32`, arithmetic wraps modulo 2^32, `sdiv`/`srem` are signed and truncate toward zero, and the optimizer folds constants with wrapping arithmetic (division by zero and `INT_MIN / -1` are left for run time, where they fault as on real hardware). Semantic analysis rejects literals above `INT_MAX`; `-2147483648` is accepted as a special case.

The x86-64 backend keeps every value in canonical form: the 64-bit sign extension of its 32-bit value, in registers and in frame slots. Bitwise operations, comparisons and booleans preserve that form for free; `add`, `sub`, `mul`, `neg` and `sdiv`/`srem` re-canonicalise with `movsxd` (division uses a genuine 32-bit `idiv`). Because the System V ABI leaves the upper half of 32-bit arguments and results undefined, parameters are read as sign-extended dwords and call results are sign-extended after the call, so the generated code does not depend on its callers or callees keeping them clean.

Testing consequence: a missing `movsxd` is only visible when the operation and the observing comparison or division are in the same function, since the call-result sign extension would otherwise repair the value. `tests/e2e.rs::int_is_32_bits_and_wraps` is written that way on purpose.

## Debugging output

`--dump-ast`, `--dump-ir[=lowered|ssa|opt]` and `--dump-regalloc` (see the README) print the compiler's intermediate state. `src/irdump.rs` renders values as `%N`, blocks as `bbN`, and phi nodes as `phi [bbPRED: %VALUE], ...`. `--dump-regalloc` appends the register or spill slot and the live interval (in linearised instruction positions) of every defined value, which is the quickest way to debug the allocator.

