# Decision: product types (type definitions)
**ID:** type-definitions
**Status:** open
**Date opened:** 2026-03-16
**Date done:** —
**Affects:** lexer, parser, eval, syntax, stdlib

## Question
How should KataScript define product types (structs) — syntax, field access, construction, and mutability?

## Context
KataScript has sum types (`enum`) but no product types. The `type` keyword is reserved in the 4-char keyword family (`func`/`type`/`kind`/`with`/`enum`) and referenced in the [type-system proposal](type-system.md) as the construct for concrete types. Product types are a prerequisite for method dispatch ([prop: method-dispatch](method-dispatch.md)) and iteration ([prop: iteration](iteration.md)) — an iterator needs to carry state (position, collection reference), and methods need a `self` that has named fields.

Currently, the only composite data in KataScript is enum variants, which carry positional fields. There's no way to define a type with named fields, construct it, or access those fields. This blocks:
- Iterator state objects
- User-defined data modeling (Point, Vec2, Config, etc.)
- Method dispatch (methods need a receiver with known structure)
- The `kind` system (conformance requires inspectable field/method sets)

The `TypeRegistry` already handles `TypeDef::Prim` and `TypeDef::Enum`. Product types will add a third variant.

## Alternatives

### Option A: `type` with named fields (record style)
```ks
type Point {
    x: Float,
    y: Float,
}

let p = Point { x: 1.0, y: 2.0 }
print(p.x)  // 1.0
```
Construction uses the type name + `{ field: value }` syntax. Fields are accessed with dot notation (already parsed as `Expr::Attr`).

**Pros:** Familiar (Rust structs, Go structs, TypeScript interfaces). Named fields are self-documenting. Dot access already works in the parser. Construction syntax reuses map-like `{ k: v }` braces — unambiguous because it follows a type name.
**Cons:** Requires the parser to distinguish `TypeName { ... }` (construction) from `{ k: v }` (map literal). Field order in construction — require all fields? Allow partial? Default values?

### Option B: `type` with positional fields (tuple-struct style)
```ks
type Point(Float, Float)

let p = Point(1.0, 2.0)
// access by index? by generated name?
```
Construction looks like a function call. Access is positional.

**Pros:** Minimal syntax. Consistent with enum variant construction (`Some(1)`). Easy to parse — same as a call expression.
**Cons:** Positional fields don't scale — `Point(1.0, 2.0)` is fine, but `Config(true, 8080, "localhost", nil)` is unreadable. No names to access by. Enum variants already cover this shape — what does `type` add?

### Option C: Both named and positional forms
```ks
type Pair(Int, Int)              // positional (tuple-struct)
type Point { x: Float, y: Float } // named (record)
```
Two forms under the same keyword.

**Pros:** Flexibility — use positional for small wrappers, named for real data. Covers both Rust `struct Foo(T)` and `struct Foo { x: T }` patterns.
**Cons:** Two construction syntaxes, two access patterns. More complexity for limited benefit — enum variants already serve the positional case.

### Option D: Named fields only, positional construction sugar
```ks
type Point {
    x: Float,
    y: Float,
}

let p = Point { x: 1.0, y: 2.0 }  // full construction
let q = Point(1.0, 2.0)            // positional sugar (field order)
```
Named fields are the canonical form. Positional construction is sugar that assigns arguments to fields in declaration order.

**Pros:** Named fields for clarity, positional for brevity. One field access mechanism (dot). Construction flexibility.
**Cons:** Positional sugar can be confusing if field order isn't obvious. Two construction paths to explain.

## Discussion
**Current state (2026-03-16):** `TypeDef` has `Prim` and `Enum` variants. `Value` has `Enum { type_id, variant_idx, fields }` for enum instances. The parser handles `Expr::Attr` (dot access) and `Expr::Call` (function calls). `TypeRegistry` manages registration, lookup, and generic instantiation.

**Named vs positional:** Enum variants already provide positional-field types. `Opt.Some(1)` is a value carrying positional data. Product types should add something new — named fields. This argues for Option A or D.

**Construction syntax:** `Point { x: 1.0, y: 2.0 }` after a type name is unambiguous — the parser sees `Ident LBrace` where the ident resolves to a type. Bare `{ k: v }` in expression position is a map (per [spec: block-syntax](../spec/block-syntax.md)). No ambiguity.

**Generics:** Product types should support type parameters: `type Pair[A, B] { fst: A, snd: B }`. The generic instantiation machinery in `TypeRegistry` already handles this for enums — it can be generalized.

**Mutability:** Two models:
1. **Immutable by default, explicit mutation** — `p.x = 3.0` is an error unless `p` was declared with `let mut` or similar. Functional style.
2. **Mutable if the binding is mutable** — if `let p = ...`, then `p.x = 3.0` works because `let` bindings are reassignable (per current semantics where `=` is reassignment). Field mutation follows variable mutation.

KataScript currently allows `x = new_value` for reassignment. Extending this to `p.x = new_value` is natural — it's just assignment to an lvalue that happens to be a field access. This favors model 2.

**Implementation in TypeRegistry:** A new `TypeDef::Struct` variant:
```rust
TypeDef::Struct {
    name: String,
    type_params: Vec<String>,
    fields: IndexMap<String, TypeExpr>,
}
```
And a corresponding `TypeDef::StructInstance` for concrete instantiations (paralleling `EnumInstance`). Or, the instantiation machinery could be unified across enums and structs.

**Value representation:** A new `Value::Struct { type_id: TypeId, fields: IndexMap<String, Value> }` or store fields as a `Vec<Value>` with name resolution through the registry. Vec is cheaper; IndexMap is more direct for named access.

**Open sub-questions:**
- Default field values? `type Config { port: Int = 8080 }` — useful but adds complexity. Defer.
- Visibility? Public/private fields? Defer to module system.
- Destructuring? `let { x, y } = point` — nice but needs pattern matching. Defer.
- Can a `type` be empty? `type Unit {}` — yes, it's a zero-field product type.

## Decision
<!-- blank while open -->

## References
- [prop: type-system](type-system.md) — two-layer type architecture, `type` keyword reserved
- [prop: method-dispatch](method-dispatch.md) — methods need receiver types with structure
- [prop: iteration](iteration.md) — iterators need state-carrying types
- [spec: block-syntax](../spec/block-syntax.md) — `{ k: v }` reserved for maps; `Type { k: v }` for construction
- [spec: func-vs-fn](../spec/func-vs-fn.md) — 4-char keyword family
- Rust structs — named fields, positional tuple-structs, `impl` blocks
- Python dataclasses — named fields, construction, default values
- Go structs — named fields, zero values, embedding
