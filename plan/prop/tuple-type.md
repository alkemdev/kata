# Decision: tuple type
**ID:** tuple-type
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** lexer, parser, interpreter

## Question
Should KataScript have a tuple type, and what syntax should it use?

## Context
There's no lightweight way to group values without defining a `kind`. Returning multiple values from a function requires defining a struct. Iteration over maps needs a Pair/Entry type. Destructuring assignment doesn't exist.

## Alternatives

### Option A: `(a, b, c)` syntax — anonymous product type
Parens create tuples: `let pos = (1.0, 2.0)`. Access via `pos.0`, `pos.1` or destructuring `let (x, y) = pos`.
**Pros:** Universal syntax. Familiar. Lightweight.
**Cons:** Parens already used for grouping and call args. Parser disambiguation needed.

### Option B: Named tuples only — just use `kind`
No special tuple type. Use `kind Pair[A, B] { fst: A, snd: B }` in stdlib.
**Pros:** No new syntax. Consistent with existing type system.
**Cons:** Verbose. Every ad-hoc grouping needs a type definition.

### Option C: Tuple as builtin prim
`Value::Tuple(Vec<Value>)` with `(a, b)` syntax and `.0`/`.1` access.
**Pros:** Fast, simple.
**Cons:** Special-cased. Not a kind or enum — doesn't participate in the type system the same way.

## Discussion
The Map type needs a key-value pair for iteration. Function multiple returns need some grouping. Destructuring assignment is a natural companion.

If we add tuples, destructuring `let (x, y) = point` should work in let bindings and match patterns.

## Decision
<!-- blank while open -->

## References
- Python tuples, Rust tuples, Swift tuples
- Haskell `(,)` type
