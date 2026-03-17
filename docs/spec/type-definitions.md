# Decision: product types (kind definitions)
**ID:** type-definitions
**Status:** done
**Date opened:** 2026-03-16
**Date done:** 2026-03-16
**Affects:** lexer, parser, eval, syntax, stdlib

## Question
How should KataScript define product types (structs) — syntax, field access, construction, and mutability?

## Context
KataScript has sum types (`enum`) but no product types. The `kind` keyword defines concrete product types in the keyword family (`func`/`kind`/`enum`/`type`/`impl`/`with`). Product types are a prerequisite for method dispatch ([prop: method-dispatch](../../plan/prop/method-dispatch.md)) and iteration ([prop: iteration](../../plan/prop/iteration.md)) — an iterator needs to carry state (position, collection reference), and methods need a `self` that has named fields.

Currently, the only composite data in KataScript is enum variants, which carry positional fields. There's no way to define a type with named fields, construct it, or access those fields. This blocks:
- Iterator state objects
- User-defined data modeling (Point, Vec2, Config, etc.)
- Method dispatch (methods need a receiver with known structure)
- Abstract type conformance (requires inspectable field/method sets)

The `TypeRegistry` already handles `TypeDef::Prim` and `TypeDef::Enum`. Product types will add a third variant.

## Alternatives

### Option A: `kind` with named fields (record style)
```ks
kind Point {
    x: Float,
    y: Float,
}

let p = Point { x: 1.0, y: 2.0 }
print(p.x)  // 1.0
```
Construction uses the type name + `{ field: value }` syntax. Fields are accessed with dot notation (already parsed as `Expr::Attr`).

**Pros:** Familiar (Rust structs, Go structs, TypeScript interfaces). Named fields are self-documenting. Dot access already works in the parser. Construction syntax reuses map-like `{ k: v }` braces — unambiguous because it follows a type name.
**Cons:** Requires the parser to distinguish `TypeName { ... }` (construction) from `{ k: v }` (map literal). Field order in construction — require all fields? Allow partial? Default values?

### Option B: `kind` with positional fields (tuple-struct style)
```ks
kind Point(Float, Float)

let p = Point(1.0, 2.0)
// access by index? by generated name?
```
Construction looks like a function call. Access is positional.

**Pros:** Minimal syntax. Consistent with enum variant construction (`Some(1)`). Easy to parse — same as a call expression.
**Cons:** Positional fields don't scale — `Point(1.0, 2.0)` is fine, but `Config(true, 8080, "localhost", nil)` is unreadable. No names to access by. Enum variants already cover this shape — what does `kind` add?

### Option C: Both named and positional forms
```ks
kind Pair(Int, Int)              // positional (tuple-struct)
kind Point { x: Float, y: Float } // named (record)
```
Two forms under the same keyword.

**Pros:** Flexibility — use positional for small wrappers, named for real data. Covers both Rust `struct Foo(T)` and `struct Foo { x: T }` patterns.
**Cons:** Two construction syntaxes, two access patterns. More complexity for limited benefit — enum variants already serve the positional case.

### Option D: Named fields only, positional construction sugar
```ks
kind Point {
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

**Construction syntax:** `Point { x: 1.0, y: 2.0 }` after a type name is unambiguous — the parser sees `Ident LBrace` where the ident resolves to a type. Bare `{ k: v }` in expression position is a map (per [spec: block-syntax](block-syntax.md)). No ambiguity.

**Generics:** Product types should support type parameters: `kind Pair[A, B] { fst: A, snd: B }`. The generic instantiation machinery in `TypeRegistry` already handles this for enums — it can be generalized.

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
- Default field values? `kind Config { port: Int = 8080 }` — useful but adds complexity. Defer.
- Visibility? Public/private fields? Defer to module system.
- Destructuring? `let { x, y } = point` — nice but needs pattern matching. Defer.
- Can a `kind` be empty? `kind Unit {}` — yes, it's a zero-field product type.

## Decision
**Chosen:** Option A — named fields only (record style)
**Rationale:** Enum variants already cover positional fields. Product types should add named, self-documenting fields. One construction syntax (`Type { field: value }`) avoids ambiguity with function calls and enum variant construction. Positional field support may be added later as a separate extension.
**Consequences:**
- `kind Point { x: Float, y: Float }` syntax; `kind` keyword in lexer (renamed from `type` in 2026-03-17 taxonomy redesign)
- Construction: `Point { x: 1.0, y: 2.0 }` — all fields required, no defaults
- Field access: `p.x` via existing `Expr::Attr`
- Field mutation: `p.x = 3.0` follows existing reassignment semantics (binding must be reassignable)
- Generics supported: `kind Pair[A, B] { fst: A, snd: B }` — reuses `TypeRegistry` instantiation
- Empty types allowed: `kind Unit {}`
- Deferred: default field values, destructuring (lands with `match`), positional construction sugar, visibility/privacy

## References
- [prop: type-system](../../plan/prop/type-system.md) — two-layer type architecture
- [prop: method-dispatch](../../plan/prop/method-dispatch.md) — methods need receiver types with structure
- [prop: iteration](../../plan/prop/iteration.md) — iterators need state-carrying types
- [spec: block-syntax](block-syntax.md) — `{ k: v }` reserved for maps; `Type { k: v }` for construction
- [spec: func-vs-fn](func-vs-fn.md) — keyword family
- Rust structs — named fields, positional tuple-structs, `impl` blocks
- Python dataclasses — named fields, construction, default values
- Go structs — named fields, zero values, embedding
