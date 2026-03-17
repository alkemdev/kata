# Decision: func vs fn
**ID:** func-vs-fn
**Status:** done
**Date opened:** 2026-03-14
**Date done:** 2026-03-14
**Affects:** lexer, parser, syntax

## Question
Should KataScript use `func` or `fn` as the function keyword?

## Context
KataScript launched with `fn` as the function keyword (matching Rust). During bootstrap it was switched to `func`. The revert cost is low — it's a single token change in the lexer and a find-replace across fixtures. `lexer.rs:244` explicitly documents `fn` as a plain identifier now. Left open because the original first instinct was `fn` and the design register hasn't been settled.

## Alternatives

### Option A: `func`
**Pros:** Full English word; readable without Rust background; already implemented; consistent with the direction of removing terseness (no semicolons, `ret` is still short but obvious).
**Cons:** 4 chars vs 2; less terse; not the original instinct.

### Option B: `fn`
**Pros:** Matches Rust; more terse; visually distinct from variable names; was the original first instinct.
**Cons:** Reverting adds git noise; could be mistaken for a short variable name by readers unfamiliar with Rust conventions.

## Discussion
**Current state (2026-03-14):** `Token::Func` is live in the lexer. `fn` was explicitly demoted — `lexer.rs:244` has a test asserting `fn` lexes as `Ident`, with the comment "fn is no longer a keyword". The parser has no function definition production yet, so neither keyword does anything at runtime. This decision gates the func feature implementation.

The meta-question is register: is KataScript a teaching/demo language readable to anyone, or a personal compiler workbench where Rust conventions are assumed?

`fn` is 2 chars; in practice it reads as: `fn greet() { ... }` vs `func greet() { ... }`. In a Rust-heavy context `fn` is immediately parsed as "function keyword" by the eye. Outside that context it could look like a short variable name, though in practice it appears before `name()` so the call site disambiguates.

`func` costs 2 extra chars per definition, which matters more in a REPL or dense scripting context than in a file. Against `ret` (3 chars) and no semicolons, the language is trending terse — `func` is the outlier.

The `ret` decision set a precedent: "kata intentionally diverges from verbose keywords where a shorter form is unambiguous." `fn` fits that pattern. `func` does not.

## Decision
**Chosen:** Option A — `func`.
**Rationale:** 4-character keyword consistency: kata anticipates `func`/`type`/`kind`/`decl` as a family of declaration keywords. Uniform length makes the language feel intentional rather than borrowed from Rust. The `ret` precedent favors terseness for *statement* keywords; declaration keywords are a different register.
**Consequences:**
- `Token::Func` stays; `fn` remains a plain identifier
- All fixture files already use `func` — no changes needed
- Future declaration keywords should target 4 characters where possible

## References
- `docs/phil/vision.md` — vision doc
- `katars/src/ks/lexer.rs:244` — `fn` documented as plain identifier
