# Decision: const bindings
**ID:** const-bindings
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** lexer, parser, interpreter

## Question
Should KataScript have immutable bindings via `const`, and what semantics should they have?

## Context
`let` creates mutable bindings — any variable can be reassigned. There's no way to declare a binding as immutable. The roadmap originally listed `const` but struck it through, suggesting it was considered and deferred.

Immutable bindings prevent accidental reassignment and communicate intent. In a language with value semantics (copy-in/copy-out), immutability is simpler than in reference-based languages.

## Alternatives

### Option A: `const` keyword — shallow immutability
`const x = 42` — `x` cannot be reassigned. Fields of `x` can still be mutated via `x.field = val` (copy-in/copy-out still works, but the binding itself can't be overwritten).
**Pros:** Simple. Prevents accidental `x = other_thing`.
**Cons:** Shallow — struct fields are still mutable. Might confuse users.

### Option B: `let` is immutable by default, `mut` for mutable
Flip the default: `let x = 42` is immutable, `let mut x = 42` is mutable. Rust model.
**Pros:** Safer defaults. Encourages immutability.
**Cons:** Breaking change — all existing code uses `let` for mutable bindings. Migration burden.

### Option C: No const — defer
Keep `let` as the only binding form. Immutability isn't critical for a scripting language.
**Pros:** Simpler language. No migration.
**Cons:** No way to prevent accidental reassignment.

### Option D: `val` for immutable, `let` stays mutable
New keyword `val x = 42` for immutable. `let` unchanged.
**Pros:** No breaking change. Kotlin precedent.
**Cons:** Another keyword.

## Discussion
The scripting language context favors simplicity. `const` (Option A) is lowest-friction — it's additive, doesn't break existing code, and the concept is universally understood.

Deep immutability (preventing field mutation on const bindings) could come later via a freeze mechanism, but shallow const is the 80% solution.

## Decision
<!-- blank while open -->

## References
- Rust `let` vs `let mut`
- Kotlin `val` vs `var`
- JavaScript `const` vs `let`
