# Compiler architecture

```text
CLI -> lexer -> parser/AST -> semantic analysis/name resolution -> x86-64 machine code -> ELF64 executable
```

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and short-circuit logical operators.

The semantic pass maintains a stack of lexical scopes, resolves each use to the nearest visible declaration, rejects undeclared uses and same-scope redeclarations, and rewrites declarations to unique internal names. This lets the backend assign separate stack slots to shadowed locals. Definite-assignment checking and type checking are not implemented.

The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; ordinary binary expressions save the left operand on the stack. Comparisons use signed x86 condition codes and normalize results to 0 or 1. `&&` and `||` emit conditional branches, so the right operand is evaluated only when required. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

This prototype is not C-conforming yet. Function calls, the C ABI, preprocessing, pointers, arrays, structures, floating point, and multiple translation units remain future work.

Next: add definite-assignment diagnostics and a target-independent typed IR with basic blocks.
