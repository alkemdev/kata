# Decision: destructuring bindings
**ID:** destructuring
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** parser, interpreter

## Question
Should `let` support destructuring patterns?

## Context
Match expressions support pattern destructuring (`Val(x) -> ...`), but `let` doesn't. Users can't write `let Val(x) = some_opt` or `let Point { x, y } = point`.

## Alternatives

### Option A: `let` with patterns
`let Val(x) = expr` — panics if pattern doesn't match (like `!`). For fallible destructuring: `let Val(x) = expr else { bail }`.
**Pros:** Concise. Natural extension of match patterns. Rust `let-else` is proven.
**Cons:** What happens on pattern mismatch? Panic? That makes `let` fallible.

### Option B: `let` with match sugar
`let x = match expr { Val(v) -> v, _ -> panic("...") }` — no new syntax, just a pattern.
**Pros:** Already works with current syntax.
**Cons:** Verbose.

### Option C: `let` patterns with refutability checking
Irrefutable patterns (struct destructuring) always work: `let Point { x, y } = point`. Refutable patterns (enum variants) require `else`: `let Val(x) = expr else { default }`.
**Pros:** Safe. Compiler can distinguish refutable from irrefutable.
**Cons:** Refutability checking adds complexity. Dynamic language may not have enough type info.

## Discussion
Struct destructuring (`let { x, y } = point`) is the highest-value case — it's always safe (all fields exist). Enum destructuring is trickier because it can fail.

The `else` clause for fallible patterns is elegant: `let Val(x) = opt else { ret default }`.

## Decision
<!-- blank while open -->

## References
- Rust `let-else`, irrefutable patterns
- JavaScript destructuring: `const { x, y } = obj`
- Python unpacking: `x, y = point`
