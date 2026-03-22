# Decision: Range type and `..` operator
**ID:** range-type
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** lexer, parser, stdlib

## Question
Should KataScript have a Range type with `..` syntax, and how should it integrate with for loops and slicing?

## Context
Currently there's no way to write `for i in 0..10`. Users must define their own IntRange kind (seen in tests). A standard Range type would enable idiomatic numeric loops and array slicing (`arr[1..3]`).

## Alternatives

### Option A: `Range` kind in std.core
`kind Range { start: Int, end: Int }` with `impl Range as ToIter[Int]`. The `..` operator is syntactic sugar: `a..b` → `Range { start: a, end: b }`.
**Pros:** Simple, consistent with existing patterns. No special runtime support.
**Cons:** Int-only. Float ranges or open ranges need more design.

### Option B: Generic `Range[T]`
`kind Range[T] { start: T, end: T }` — works with any ordered type.
**Pros:** General. Float ranges, char ranges, etc.
**Cons:** Iteration needs T to support increment. Requires a `Step` or `Succ` protocol.

### Option C: Defer — just provide `range(start, end)` function
A builtin function that returns an iterable. No new syntax.
**Pros:** Minimal. No lexer/parser changes.
**Cons:** `for i in range(0, 10)` is more verbose than `for i in 0..10`.

## Discussion
The `..` token needs to be added to the lexer. Must not conflict with `.` (attribute access) — the lexer needs to distinguish `a.b` from `a..b`. Longest-match should handle it: `..` is lexed before `.`.

Also consider: `..=` for inclusive ranges? `..` for exclusive (Rust convention)?

Slicing integration: `arr[1..3]` passes a Range to `get_item`. Arr's GetItem implementation checks if the key is Int or Range and returns a single element or a sub-array.

## Decision
<!-- blank while open -->

## References
- Rust `std::ops::Range`, `..` and `..=` syntax
- Python `range()` builtin + slice syntax
- Existing IntRange in tests: `tests/ks/for/break_in_for.ks`
