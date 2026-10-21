# Compiler architecture

```text
CLI -> lexer -> parser/AST -> semantic analysis -> target-independent integer IR -> constant folding -> native x86-64 backend -> ELF64 executable
```

The frontend and semantic checks operate on the resolved AST. `src/ir.rs` lowers that AST to a separate integer IR with explicit local, constant, unary, binary, store, branch, loop and return nodes. Its optimizer folds constant arithmetic, bitwise operations, comparisons and logical expressions where evaluation is safe. It uses checked arithmetic, so overflow, division by zero, and signed division overflow are left for runtime rather than being folded. Short-circuit simplification avoids even optimizing an unreachable right operand when the left constant determines `&&` or `||`.

The backend currently has a temporary adapter that converts optimized IR back to the existing AST-shaped form before x86-64 emission. This isolates the new representation and lets optimizations run now; a follow-up should replace the adapter with direct IR instruction/basic-block code generation.

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and short-circuit logical operators.

The semantic pass maintains lexical scopes, resolves uses to the nearest visible declaration, rejects undeclared uses and same-scope redeclarations, and rewrites declarations to unique internal names. Definite-assignment analysis tracks initialized locals, intersects branch outputs, and assumes loops may execute zero times. The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; ordinary binary expressions save the left operand on the stack. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

This prototype is not C-conforming yet. Function calls, the C ABI, preprocessing, pointers, arrays, structures, floating point, and multiple translation units remain future work.

Next: direct IR-to-machine-code lowering, basic blocks, and explicit control-flow edges.