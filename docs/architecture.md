# Compiler architecture

## Pipeline

```text
CLI driver
  -> lexer (tokens with source offsets)
  -> parser (bootstrap AST)
  -> semantic validation (next milestone)
  -> typed IR (planned)
  -> optimizer (planned)
  -> x86-64 instruction emitter
  -> ELF64 executable writer
```

## Current AST

The expression tree has integer literals, unary plus/minus/bitwise-not, and binary arithmetic and bitwise operators. The parser uses Pratt binding powers to encode precedence. The initial translation-unit grammar deliberately accepts only one `int main()` or `int main(void)` function containing a single `return expression;` statement.

## Backend invariants

- Target is System V x86-64 Linux.
- The generated process returns the expression value through Linux syscall 60 (`exit`); the low 8 bits are observed as the process exit status.
- Expression results are currently represented in RAX.
- Binary-expression lowering saves the left operand on the machine stack while emitting the right operand.
- ELF output is a minimal ELF64 executable with one executable/loadable segment. This is not yet a general object-file writer and does not support relocations or external symbols.

## Next design step

Introduce statements and declarations in the AST, then a typed IR with explicit basic blocks. Keep source-language rules (C vs C++) out of the target backend. Build separate C and C++ semantic modes over shared token infrastructure and common low-level IR.
