# Feature: enums

**Status:** draft
**Tracking:** Phase 2a prerequisite for `Opt[T]`, `Res[T, E]`

---

## Summary

Sum types (tagged unions) with optional data payloads and generic type parameters.

---

## Syntax

```bnf
(* NEW *)
enum_def    = 'enum' IDENT type_params? '{' variant_list '}'
type_params = '[' IDENT (',' IDENT)* ']'
variant_list = variant (',' variant)* ','?
variant     = IDENT                          (* unit variant *)
            | IDENT '(' type_expr (',' type_expr)* ')'   (* data variant *)

(* NEW *)
type_expr   = IDENT                          (* concrete: Int, Str *)
            | IDENT '[' type_expr (',' type_expr)* ']'   (* generic: List[Int] *)

(* CHANGED — was: stmt = 'ret' expr ';'? | expr ';'? *)
stmt        = enum_def
            | 'ret' expr ';'?
            | expr ';'?

(* NEW — fully qualified variant construction *)
expr        = IDENT type_args? '.' IDENT '(' (expr (',' expr)*)? ')'   (* Opt[Int].Some(42) *)
            | IDENT type_args? '.' IDENT                                (* Opt[Int].None *)
            | ...existing...
type_args   = '[' type_expr (',' type_expr)* ']'
```

---

## Semantics

### Definition

`enum` defines a new named type with a fixed set of variants. Each variant is either:
- **Unit** — carries no data (`None`, `Err`)
- **Data** — carries one or more values (`Some(T)`, `Ok(T)`)

Type parameters (`[T]`, `[T, E]`) are available within the variant definitions.

```ks
enum Opt[T] {
    Some(T),
    None,
}

enum Res[T, E] {
    Ok(T),
    Err(E),
}

enum Color {
    Red,
    Green,
    Blue,
}
```

### Construction

Variants are constructed via fully qualified dot syntax:

```ks
let x = Opt[Int].Some(42)
let y = Opt[Int].None
let c = Color.Red
```

The enum name acts as a namespace. Variants are not injected into the enclosing scope — `Some(42)` alone is not valid. Non-generic enums don't need type args (`Color.Red`), but generic enums require them (`Opt[Int].Some(42)`) for now. Type inference may relax this later.

### Runtime representation

An enum value carries:
- The enum type name (e.g., `"Opt"`)
- The variant name (e.g., `"Some"`)
- The payload values (possibly empty)

In the `Value` enum in `eval.rs`:
```rust
Value::Enum {
    type_name: String,
    variant: String,
    fields: Vec<Value>,
}
```

### Display

`print(Opt[Int].Some(42))` → `Some(42)`
`print(Opt[Int].None)` → `None`
`print(Color.Red)` → `Red`

### Type rules

- Generic enums require type arguments at construction: `Opt[Int].Some(42)`.
- Wrong number of type args is an error: `Opt.Some(42)` when `Opt` has 1 type param → error.
- Wrong number of arguments is an error: `Opt[Int].Some(1, 2)` → error.
- Unknown variant is an error: `Opt[Int].Maybe(42)` → error.
- Unknown enum is an error: `Foo.Bar` → error.

### Error conditions

| Condition | Error message fragment |
|-----------|----------------------|
| Unknown enum name | `undefined type 'Foo'` |
| Unknown variant | `'Opt' has no variant 'Maybe'` |
| Wrong argument count | `'Some' expects 1 argument, got 2` |
| Data variant called with no args | `'Some' expects 1 argument, got 0` |
| Unit variant called with args | `'None' expects 0 arguments, got 1` |

---

## Examples

### Happy path

```ks
enum Direction {
    North,
    South,
    East,
    West,
}

let d = Direction.North
print(d)
```

Expected stdout:
```
North
```

```ks
enum Opt[T] {
    Some(T),
    None,
}

let x = Opt[Int].Some(42)
let y = Opt[Int].None
print(x)
print(y)
```

Expected stdout:
```
Some(42)
None
```

### Error cases

```ks
enum Opt[T] {
    Some(T),
    None,
}
Opt[Int].Some(1, 2)
```

Expected stderr contains:
```
'Some' expects 1 argument, got 2
```

```ks
enum Opt[T] {
    Some(T),
    None,
}
Opt[Int].Maybe(1)
```

Expected stderr contains:
```
'Opt' has no variant 'Maybe'
```

---

## Interactions with existing features

- **Depends on `let`** — enum values need to be bound to variables to be useful. See `docs/dev/specs/let.md`.
- **Pattern matching** — destructuring enum values is the primary way to use them. Not part of this spec; will be a follow-on feature.
- **Prelude loading** — `Opt` and `Res` will be defined in `std/prelude.ks` once enums land.

---

## Non-goals / deferred

- **Pattern matching / `match`** — separate feature spec
- **Methods on enums** — requires method dispatch system
- **Exhaustiveness checking** — requires static analysis, deferred
- **Impl blocks** — deferred to `type`/`kind` system
- **Nested generics** — `Opt[List[Int]]` — deferred until needed
- **Type inference** — `Opt.Some(42)` inferring `Opt[Int]` — deferred
- **Type parameter bounds** — `[T: Display]` — deferred to `kind` system

---

## Done criteria

- [ ] Spec reviewed and finalized
- [ ] Conformance tests written and failing (red)
- [ ] Lexer updated — `Token::Enum` keyword added
- [ ] AST updated — `Stmt::EnumDef`, `Expr::EnumVariant` added, serde derives present
- [ ] Parser updated — BNF comment matches implementation
- [ ] Evaluator updated — enum definitions stored, variant construction works
- [ ] `cargo test` green
- [ ] `--dump-ast | jq .` works for enum definitions and variant construction
- [ ] No `panic!` or `unwrap` on user input in eval path
- [ ] `Opt[T]` and `Res[T, E]` definable and constructable in `.ks`
