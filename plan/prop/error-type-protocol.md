# Decision: Error type protocol and error composition
**ID:** error-type-protocol
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** stdlib, interpreter

## Question
Should KataScript define an `Error` interface, and how should different error types compose when using `?`?

## Context
The error handling spec chose `Res[T, E]` with `?` propagation as-is (no auto-conversion). This means if function A returns `Res[Int, ParseError]` and function B returns `Res[Int, IoError]`, using `?` inside B on a call to A will fail the return type check because `ParseError != IoError`.

Rust solves this with `From<T>` trait and `Box<dyn Error>`. KataScript needs its own answer.

## Alternatives

### Option A: `From[T]` protocol for `?`
Define `type From[T] { func from(val: T): Self }`. When `?` propagates an Err, check if a `From` conversion exists from the inner error type to the outer function's error type.
**Pros:** Principled. Composable. Rust-proven.
**Cons:** Complex. Requires the interpreter to introspect the enclosing function's return type at `?` evaluation time.

### Option B: `Error` interface as common base
Define `type Error { func message(self): Str }`. Functions return `Res[T, Error]` where Error is a protocol. Any type implementing Error can be the E.
**Pros:** Simple. One error type to rule them all.
**Cons:** Loses type information. Can't pattern match on specific error types without downcasting.

### Option C: Enum-based error composition
Users define error enums: `enum AppError { Parse(ParseError), Io(IoError) }`. Manual wrapping.
**Pros:** Fully typed. Pattern matchable. No magic.
**Cons:** Boilerplate for every error boundary.

### Option D: Defer
Keep `?` propagation as-is. Users manually match and re-wrap errors. Wait for real-world pain to guide the design.
**Pros:** No complexity now. Design informed by usage.
**Cons:** Error handling stays verbose.

## Discussion
This is a Phase 3+ concern. The current error handling is functional for simple programs. The composition question matters when programs grow and have multiple error domains.

## Decision
<!-- blank while open -->

## References
- [spec: error-handling](../../docs/spec/error-handling.md)
- Rust `From` trait, `thiserror`, `anyhow`
- Go error wrapping with `%w`
