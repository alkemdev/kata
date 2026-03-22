# Decision: closure syntax and higher-order function ergonomics
**ID:** closures-and-lambdas
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** lexer, parser, ast

## Question
Should KataScript have a short lambda syntax, and how should closures interact with higher-order functions?

## Context
Functions are first-class values. `func` definitions create closures that capture their lexical environment. But there's no short anonymous function syntax — every function needs the `func name(params) { body }` form. This makes higher-order patterns verbose:

```ks
# current: must name the function
func double(x: Int): Int { ret x * 2 }
arr.map(double)

# desired: inline anonymous function
arr.map(|x| x * 2)
```

This blocks ergonomic use of `.map()`, `.filter()`, `.and_then()`, `.or_else()` on Opt/Res/Arr.

## Alternatives

### Option A: Rust-style `|params| expr`
`|x| x * 2`, `|x, y| x + y`, `|x: Int| -> Int { ret x * 2 }` for typed/multi-statement.
**Pros:** Concise. Familiar from Rust. Natural in postfix chains.
**Cons:** `|` is not currently a token. Might conflict with future pipe operator.

### Option B: Arrow syntax `(params) -> expr`
`(x) -> x * 2`, `(x, y) -> x + y`. Multi-statement: `(x) -> { ... }`.
**Pros:** Familiar from JS/CoffeeScript. `->` already exists as a token (match arms).
**Cons:** `->` is used in match — could create parser ambiguity in certain contexts.

### Option C: Backslash `\x -> expr`
Haskell-inspired: `\x -> x * 2`.
**Pros:** Unambiguous. Short.
**Cons:** Unusual. `\` has escape-sequence connotations.

### Option D: `func` keyword, allow anonymous
`func(x) { ret x * 2 }` — same keyword, just drop the name.
**Pros:** No new syntax. Consistent with existing `func`.
**Cons:** Verbose. `func(x) { ret x * 2 }` is not much shorter than naming it.

## Discussion
The choice interacts with whether the language gets `.map()`, `.filter()` etc. Without lambdas, those methods are painful to use. With lambdas, they become the primary way to compose behavior.

Key constraint: KataScript is expression-oriented. A lambda should be an expression that evaluates to a function value.

Also consider: should lambdas support type annotations? If so, the syntax needs to accommodate them.

## Decision
<!-- blank while open -->

## References
- Rust closures: `|x| x * 2`
- Python lambdas: `lambda x: x * 2`
- JS arrows: `(x) => x * 2`
- Current function values: `interpreter.rs` Value::Func
