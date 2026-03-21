# Decision: nil vs Option[T]
**ID:** nil-option
**Status:** open
**Date opened:** 2026-03-14
**Date done:** —
**Affects:** eval, syntax, stdlib

## Question
Should KataScript use `nil` only, `Opt[T]` only, or both?

## Context
KataScript already has `nil` as a runtime value (`Value::Nil`). The type system design introduces `Opt[T]` as a builtin type — a plain enum defined in KS itself (`enum Opt[T] { Val(T), Non }`). The question is whether these coexist, and if so, how.

This is Hoare's "billion-dollar mistake" territory. Dynamically-typed languages traditionally use `nil`/`null`/`Non` everywhere, which leads to null-reference errors. Statically-typed languages like Rust use `Option<T>` to make absence explicit at the type level. KataScript is dynamically typed but aspires to structural typing via `kind` — the answer here affects how much null-safety the language can offer.

## Alternatives

### Option A: `nil` only
No `Opt` type. `nil` is the universal "nothing" value. Any variable can be `nil`.
**Pros:** Simple; familiar to users of Python/Ruby/JS; no wrapper type overhead; natural for a dynamic language.
**Cons:** Billion-dollar mistake; no way to distinguish "absent" from "present but nil"; `nil` propagation bugs are silent.

### Option B: `Opt[T]` only
No `nil` value. Absence is always `Opt.Non`, presence is `Opt.Val(v)`. Pattern matching required to extract values.
**Pros:** Explicit; no null-reference errors; Rust-like safety.
**Cons:** Heavy for a scripting language; every "might not exist" case needs wrapping/unwrapping; unfamiliar to dynamic-language users; verbose without pattern matching and `?`.

### Option C: Both — `nil` as sugar for `Opt.Non`
`nil` exists as a literal but is semantically `Opt.Non`. A variable of type `T` cannot be `nil`; only `Opt[T]` can. `nil` is sugar that infers the `Opt` wrapper.
**Pros:** Ergonomic; familiar syntax with safe semantics; gradual migration path.
**Cons:** Coercion complexity — when does `T` auto-wrap to `Opt[T]`?; type inference becomes harder; two spellings for the same concept.

### Option D: Both but independent
`nil` is a value of type `Nil`. `Opt[T]` is a separate sum type. They don't alias.
**Pros:** Clean semantics; no coercion magic; `Nil` is just another type.
**Cons:** Doesn't solve nil-safety — any variable can still hold `Nil`; two absence mechanisms is confusing.

## Discussion
**Current state (2026-03-14):** `Value::Nil` exists and is used as the return value of statements and functions without explicit `ret`. It's the default "nothing" in the language.

In a dynamically-typed language, `Opt` is less compelling than in Rust because there's no compiler to enforce exhaustive matching. Without static type checking, `Opt.Val(v)` vs `nil` is just ceremony — the runtime error moves from "nil reference" to "forgot to unwrap Opt."

Key constraint: `Opt[T]` is a regular enum defined in KS (`enum Opt[T] { Val(T), Non }`), not a runtime primitive. This makes Option C (nil-as-sugar) harder — the runtime would need to know about a user-defined type to desugar `nil` into it.

However, if Phase 3 introduces `kind` (structural typing/protocols), a "Nullable" kind could unify the concept: a type conforms to `Nullable` if it can be `nil`. This would let the `kind` system provide opt-in null-safety without requiring `Opt` everywhere.

Dart's approach is interesting: null safety is built into the type system (`T` vs `T?`), but the underlying value is still `null`. This gives safety without wrapper types.

For a scripting language, Option A (`nil` only) is the pragmatic starting point. Option C is the aspirational endpoint if the type system matures. The `kind` system might make Option D viable by letting `Nil` conform to a "Nullable" kind that types can opt into.

Key tension: KataScript wants to be a scripting language (simple, dynamic) AND a type system playground (structured, safe). The answer probably evolves with the language.

## Decision
<!-- blank while open -->

## References
- [spec: type-system](../../docs/spec/type-system.md) — overall type architecture
- Hoare, "Null References: The Billion Dollar Mistake" (2009)
- Dart null safety: https://dart.dev/null-safety
- Rust `Option<T>` — no null, explicit absence
- Python `Non` — universal nil, no safety
