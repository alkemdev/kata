# Decision: pattern matching improvements
**ID:** pattern-matching-v2
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** parser, interpreter

## Question
What pattern matching features should be added beyond the current basic implementation?

## Context
Match expressions exist with four pattern types: Variant (destructuring), Literal, Wildcard (`_`), Binding (catch-all with name). This covers basic cases but lacks features that make pattern matching truly powerful.

## Missing features

### Nested patterns
`Val(Val(x))` — match through multiple layers. Currently only one level of destructuring.

### Guards
`Val(x) if x > 0 -> ...` — conditional matching. The `if` keyword after a pattern adds a boolean guard.

### Struct patterns
`Point { x, y } -> ...` — destructure struct fields by name. Currently no struct pattern matching.

### Or patterns
`Val(1) | Val(2) -> ...` — match multiple patterns in one arm. Reduces repetition.

### Exhaustiveness checking
Warn (or error) when a match doesn't cover all variants of an enum. Currently a runtime `NoMatchArm` error.

## Discussion
Nested patterns and guards are the highest-value additions. They enable idiomatic code like:

```ks
match result {
    Val(Val(x)) -> print("inner: {x}"),
    Val(Non) -> print("outer ok, inner empty"),
    Err(e) -> print("error: {e}"),
}
```

Guards enable filtering without nesting:
```ks
match age {
    n if n >= 18 -> "adult",
    n if n >= 13 -> "teen",
    _ -> "child",
}
```

Exhaustiveness checking is a static analysis feature — harder in a dynamic language but possible for enum types where all variants are known.

## Decision
<!-- blank while open -->

## References
- Rust match: nested patterns, guards, or-patterns, exhaustiveness
- Haskell case: guards, as-patterns, nested patterns
- Current implementation: `interpreter.rs` match_pattern, `ast.rs` Pattern enum
