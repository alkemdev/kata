# Decision: semicolons
**ID:** semicolons
**Status:** done
**Date opened:** 2026-03-14
**Date done:** 2026-03-14
**Affects:** lexer, parser, syntax

## Question
Should statement-terminating semicolons be required, optional, or forbidden?

## Context
The initial phase 1 implementation required semicolons. The requirement was removed after syntax test design showed optional semicolons made multi-line code cleaner. `;` is still a valid token and appears in some `.ks` fixtures.

## Alternatives

### Option A: Required
**Pros:** Unambiguous; Python-via-readline feel; explicit statement boundaries.
**Cons:** Visual noise with no semantic benefit given the current grammar; discourages multi-line readable code.

### Option B: Optional
**Pros:** Modern scripting language feel (Go, Swift, Kotlin); `.ks` files read cleanly; backwards-compatible with fixtures that already use `;`.
**Cons:** Slightly more parser complexity (optional token consumption).

### Option C: Forbidden
**Pros:** Enforces significant-whitespace style consistently.
**Cons:** Would break the phase 1 test suite; adds whitespace-sensitivity complexity; inconsistent with expression-oriented style where `;` might express sequencing.

## Discussion
The grammar has no ambiguity cases where semicolons are needed to disambiguate. Required semicolons were inherited from phase 1 scaffolding, not from a design decision. Optional is the least disruptive path and matches what modern scripting languages do.

## Decision
**Chosen:** Option B — optional semicolons.
**Rationale:** Required semicolons added visual noise with no semantic benefit given the current grammar has no ambiguity cases. Forbidden semicolons would break the phase 1 test suite and add whitespace-sensitivity complexity. Optional is the least disruptive and most flexible.
**Consequences:**
- `parse_error_missing_semicolon` test removed
- BNF in `parser.rs` shows `';'?`
- `;` remains a valid token and can appear in `.ks` files

## References
- `katars/src/ks/parser.rs` — BNF comment at top of file
