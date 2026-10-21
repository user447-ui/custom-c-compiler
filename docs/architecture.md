# Compiler architecture

```text
CLI -> lexer -> parser/AST -> name resolution during lowering -> x86-64 machine code -> ELF64 executable
```

The bootstrap grammar accepts one `int main()` or `int main(void)` function. Statements include initialized/uninitialized local `int` declarations, assignment, nested blocks, `if`/`else`, `while`, and `return`. Expressions support integer literals, local reads, unary `+`/`-`/`~`/`!`, arithmetic, bitwise operators, signed comparisons, and logical operators.

The backend allocates one 8-byte RBP-relative slot per local, emits branch placeholders, and patches relative offsets after laying out code. Expression values live in RAX; binary expressions save the left operand on the stack. Comparisons use signed x86 condition codes and normalize results to 0 or 1. Logical operators normalize both operands and combine their truth values; short-circuit evaluation is not yet implemented. The generated process exits via Linux syscall 60. Output is a minimal ELF64 executable, not a relocatable object file.

Known constraints: lexical scope/shadowing, type checking, definite assignment, function calls, and C ABI lowering are not implemented. This prototype is not C-conforming yet.

Next: add a separate semantic-analysis pass, correct lexical scopes and shadowing, then introduce a target-independent typed IR with explicit basic blocks.
