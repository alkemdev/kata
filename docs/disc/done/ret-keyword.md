# Decision: ret keyword
**ID:** ret-keyword
**Status:** done
**Date opened:** 2026-03-14
**Date done:** 2026-03-14
**Affects:** lexer, parser, syntax

## Question
Should the explicit return statement use the keyword `return` or `ret`?

## Context
Added when the `ret` statement was implemented. `return` is the default in most languages and requires no explanation. `ret` was chosen at design time as a shorter form. The choice shapes the feel of the language and is consistent with the broader question of verbosity vs. terseness.

## Alternatives

### Option A: `return`
**Pros:** Universal across languages; readable with no learning curve; no confusion.
**Cons:** 6 chars; verbose for a small scripting language; inconsistent with the direction of removing ceremony (no required semicolons).

### Option B: `ret`
**Pros:** Terse; distinct; consistent with kata's small-core aesthetic; immediately obvious to any programmer despite being non-standard.
**Cons:** Non-standard; requires learning; `return` in `.ks` files will produce a parse error (it becomes a plain identifier).

## Discussion
kata intentionally diverges from verbose keywords where a shorter, unambiguous form exists. `ret` is 3 chars and instantly understood. The tradeoff is that anyone writing `.ks` code from muscle memory may type `return` and get a confusing error. This is acceptable given the personal-workbench nature of the project.

## Decision
**Chosen:** Option B — `ret`.
**Rationale:** kata intentionally diverges from verbose keywords where a shorter form is unambiguous. `ret` is immediately obvious to any programmer; the full word `return` is unnecessary ceremony in a small scripting language. Consistent with removing semicolons and the short-keyword direction.
**Consequences:**
- `Token::Return` was removed from the lexer; `return` is now a plain identifier
- Any `.ks` code using `return` will produce a parse error (treated as an identifier in statement position)

## References
- `katars/src/ks/lexer.rs` — `Token` enum; `return` absent, `ret` present
