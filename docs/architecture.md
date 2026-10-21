# Compiler architecture

```text
CLI -> lexer -> parser/AST -> semantic analysis/name resolution + definite assignment -> x86-64 machine code -> ELF64 executable
```

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and short-circuit logical operators.

The semantic pass maintains a stack of lexical scopes, resolves each use to the nearest visible declaration, rejects undeclared uses and same-scope redeclarations, and rewrites declarations to unique internal names. A data-flow pass tracks definitely initialized locals: initializer expressions are checked before a variable is marked initialized; branch outputs are intersected; and loop bodies are checked without assuming they execute at least once. Shadowed locals receive separate stack slots. Unreachable-code analysis and type checking are not implemented.

The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; ordinary binary expressions save the left operand on the stack. Comparisons use signed x86 condition codes and normalize results to 0 or 1. `&&` and `||` emit conditional branches, so the right operand is evaluated only when required. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

This prototype is not C-conforming yet. Function calls, the C ABI, preprocessing, pointers, arrays, structures, floating point, and multiple translation units remain future work.

Next: introduce a target-independent typed IR, basic blocks, and constant folding.
