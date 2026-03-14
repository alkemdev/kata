# Feature: <name>

**Status:** draft | in-progress | done
**Tracking:** <!-- issue or roadmap ref -->

---

## Summary

One sentence describing what this feature does.

---

## Syntax

BNF delta — mark each production with `NEW` or `CHANGED`. If no grammar changes, write "No syntax changes."

```bnf
(* NEW *)
expr ::= ...

(* CHANGED — was: ... *)
stmt ::= ...
```

---

## Semantics

Describe eval behavior, type rules, and error conditions.

- **Happy path:** what happens when inputs are valid.
- **Type rules:** what types are accepted / coerced / rejected.
- **Error conditions:** list each with the expected error message fragment.

---

## Examples

### Happy path

```katascript
<!-- input -->
```

Expected stdout:
```
<!-- output -->
```

### Error cases

```katascript
<!-- input that should fail -->
```

Expected stderr contains:
```
<!-- fragment -->
```

---

## Interactions with existing features

List any features this interacts with or depends on. Note any ordering or precedence concerns.

---

## Non-goals / deferred

What is explicitly out of scope for this feature.

---

## Done criteria

- [ ] Spec reviewed and finalized
- [ ] Conformance tests written and failing (red)
- [ ] Lexer updated (if needed) — `Token` variant added
- [ ] AST updated (if needed) — serde derives present
- [ ] Parser updated — BNF comment matches implementation
- [ ] Evaluator updated — uses `out: &mut impl Write`, no `println!`
- [ ] `cargo test` green
- [ ] `--dump-ast | jq .` works for new syntax
- [ ] No `panic!` or `unwrap` on user input in eval path
- [ ] <!-- feature-specific criteria -->
