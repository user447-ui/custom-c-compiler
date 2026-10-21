# Compiler architecture

```text
CLI -> lexer -> parser/AST -> name resolution during lowering -> x86-64 machine code -> ELF64 executable
```

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`, arithmetic, and bitwise operators.

The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; binary expressions save the left operand on the stack. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

Known constraints: comparisons/logical short-circuiting, lexical scope/shadowing, type checking, definite assignment, function calls, and C ABI lowering are not implemented. This prototype is not C-conforming yet.

Next: add comparison/logical operators and a separate semantic-analysis pass, then introduce a target-independent typed IR with explicit basic blocks.
