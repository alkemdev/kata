# Feature: let

**Status:** draft
**Tracking:** Phase 2 — variables

---

## Summary

Variable binding with `let`, introducing a lexical environment to the evaluator.

---

## Syntax

```bnf
(* CHANGED — was: stmt = 'ret' expr ';'? | expr ';'? *)
stmt = 'let' IDENT '=' expr ';'?    (* NEW — variable binding *)
     | 'ret' expr ';'?
     | expr ';'?
```

---

## Semantics

- **Happy path:** `let x = <expr>` evaluates `<expr>`, binds the result to `x` in the current scope, and produces `Flow::Next`.
- **Variable lookup:** `Expr::Name("x")` looks up `x` in the environment. Currently returns an error — after this feature, it returns the bound value.
- **Shadowing:** A second `let x = ...` in the same scope rebinds `x`. No error.
- **Scope:** Lexical scoping via a scope stack. `let` binds in the innermost scope. Lookup walks outward from innermost to outermost. New scopes are pushed/popped by blocks, function bodies, etc. (those features come later, but the infrastructure is in place).

### Error conditions

| Condition | Error message fragment |
|-----------|----------------------|
| Missing initializer | parse error (no production matches) |
| Use before definition | `undefined variable 'x'` |

---

## Examples

### Happy path

```ks
let x = 42
print(x)
```

Expected stdout:
```
42
```

```ks
let greeting = "hello"
print(greeting)
```

Expected stdout:
```
hello
```

```ks
let a = true
let b = false
print(a)
print(b)
```

Expected stdout:
```
true
false
```

```ks
let x = 1
let x = 2
print(x)
```

Expected stdout:
```
2
```

### Error cases

```ks
print(x)
```

Expected stderr contains:
```
undefined variable 'x'
```

---

## Interactions with existing features

- **`ret`** — `ret x` should work if `x` is bound.
- **`Expr::Name`** — currently always errors. After this, it does environment lookup.
- **Future: blocks/scoping** — this feature uses a flat environment. Nested scopes come with `if`/`while`/`func` blocks.

---

## Non-goals / deferred

- **`const`** — immutable bindings, deferred
- **Destructuring** — `let (a, b) = ...`, deferred to pattern matching
- **Type annotations** — `let x: Int = ...`, deferred to type system
- **Block scoping** — deferred to block expressions

---

## Done criteria

- [ ] Spec reviewed and finalized
- [ ] Conformance tests written and failing (red)
- [ ] Lexer — no changes needed (`Token::Let` already exists)
- [ ] AST updated — `Stmt::Let` variant added, serde derives present
- [ ] Parser updated — BNF comment matches implementation
- [ ] Evaluator updated — `Environment` struct, variable lookup in `eval_expr`
- [ ] `cargo test` green
- [ ] `--dump-ast | jq .` works for let statements
- [ ] No `panic!` or `unwrap` on user input in eval path
- [ ] Shadowing works (re-binding same name)
