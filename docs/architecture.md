# Compiler architecture

```text
CLI -> lexer -> parser/AST -> semantic analysis -> basic-block IR -> constant folding -> CFG simplification -> dead-value elimination -> direct native x86-64 backend -> ELF64 executable
```

The frontend and semantic checks operate on the resolved AST. `src/ir.rs` lowers the AST into a target-independent control-flow graph. Each basic block contains instructions over virtual values and ends in an explicit jump, conditional branch, or return. Logical `&&` and `||` are lowered to branches so the right-hand operand executes only when required. Constant folding runs within individual blocks and uses checked arithmetic, so overflow, division by zero, and signed division overflow are not folded. Follow-up passes simplify branches with known conditions, remove unreachable blocks and remap their edges, then iteratively remove unused pure computations while preserving stores and terminators.

The native backend consumes this IR directly; the former IR-to-AST adapter has been removed. It assigns stable stack homes to local variables and virtual values, emits each block in order, and patches relative branch displacements after block offsets are known. Stack homes rather than SSA/phi nodes keep control-flow joins straightforward for this bootstrap. The generated process exits via Linux x86-64 syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and short-circuit logical operators.

The semantic pass maintains lexical scopes, resolves uses to the nearest visible declaration, rejects undeclared uses and same-scope redeclarations, and rewrites declarations to unique internal names. Definite-assignment analysis tracks initialized locals, intersects branch outputs, and assumes loops may execute zero times. The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; ordinary binary expressions save the left operand on the stack. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

This prototype is not C-conforming yet. Function calls, the C ABI, preprocessing, pointers, arrays, structures, floating point, and multiple translation units remain future work.

SSA construction with phi nodes and register allocation are intentionally deferred until the CFG optimizer has broader test coverage.