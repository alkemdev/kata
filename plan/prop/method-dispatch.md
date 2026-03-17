# Decision: method dispatch
**ID:** method-dispatch
**Status:** open
**Date opened:** 2026-03-16
**Date done:** —
**Affects:** parser, eval, syntax

## Question
How should KataScript associate functions with types and dispatch `.method()` calls?

## Context
KataScript has dot access (`Expr::Attr`) and function calls (`Expr::Call`), so `x.foo(y)` already parses as "access `foo` on `x`, then call the result with `y`." But there's no mechanism to define what `foo` means on a given type. Currently, dot access is hardcoded for:
- Enum types: `Opt.Some` produces a `VariantConstructor`
- Namespaces: `std.ops` resolves sub-namespaces and builtins

Method dispatch is needed for:
- **Iteration** ([prop: iteration](iteration.md)) — types need `.iter()`, `.next()` methods
- **Operator overloading** ([prop: operator-overloading](operator-overloading.md)) — the `kind`-based approach (Option C) requires associating operator functions with types
- **Stdlib ergonomics** — `opt.unwrap()`, `list.len()`, `str.split(",")` (see [stdlib plan](../phil/stdlib.md))
- **Product types** ([prop: type-definitions](type-definitions.md)) — types need associated functions, not just data

The dispatch mechanism must work for both `enum` and `kind` (product types), and eventually for prim types (e.g., `"hello".len()`).

## Alternatives

### Option A: `impl` blocks (Rust model)
```ks
kind Point { x: Float, y: Float }

impl Point {
    func distance(self, other: Point): Float {
        // ...
    }
}

let p = Point { x: 0.0, y: 0.0 }
p.distance(other)
```
Methods are defined in `impl` blocks attached to a type. `self` is the receiver. Dispatch looks up the method in the type's impl table.

**Pros:** Explicit association between type and methods. Clean grouping. Familiar from Rust. Separates data definition (`type`) from behavior (`impl`). Multiple `impl` blocks allowed — extensible.
**Cons:** New keyword (`impl`, 4 chars — fits the family). `self` is magic — it's the receiver, not a regular parameter. Requires a method table per type in the interpreter.

### Option B: Functions in the type body
```ks
type Point {
    x: Float,
    y: Float,

    func distance(self, other: Point): Float {
        // ...
    }
}
```
Methods are defined inline in the type definition. `self` refers to the instance.

**Pros:** Everything about a type is in one place. Less syntax than `impl`. Familiar from Python/JS classes.
**Cons:** Mixes data and behavior — harder to see the field layout at a glance. Can't add methods from outside the type definition (no extension methods). For enums, where do methods go?

### Option C: Uniform Function Call Syntax (UFCS)
```ks
func distance(p: Point, other: Point): Float {
    // ...
}

let p = Point { x: 0.0, y: 0.0 }
p.distance(other)  // sugar for distance(p, other)
```
Any function whose first parameter matches the type of the receiver can be called with dot syntax. No special `self`, no `impl` blocks.

**Pros:** No new keywords or constructs. Functions are functions — no method/function distinction. Extension methods are free — any function can be a "method." Simpler mental model.
**Cons:** Ambiguity — if two functions named `distance` take a `Point` first arg, which one wins? Scope rules become critical. Harder to see what "methods" a type has. In a dynamically-typed language, "first parameter matches" is checked at call time, not definition time.

### Option D: `impl` blocks + UFCS for extension
```ks
kind Point { x: Float, y: Float }

impl Point {
    func distance(self, other: Point): Float { ... }
}

// elsewhere — extension method via UFCS
func debug_print(p: Point) {
    print("Point(" + std.ops.str(p.x) + ", " + std.ops.str(p.y) + ")")
}

p.distance(other)     // impl lookup
p.debug_print()       // UFCS fallback
```
`impl` is the primary mechanism. UFCS provides extension methods as a fallback.

**Pros:** Best of both worlds — explicit `impl` for core methods, UFCS for ad-hoc extension. `impl` gives a clear "methods of this type" story.
**Cons:** Two dispatch paths to explain and implement. Priority rules (impl wins over UFCS? Or scope-based?).

### Option E: Protocol-only dispatch (via `kind`)
```ks
kind Iterable {
    func iter(self): Iterator
}

// types "implement" kinds by having matching methods
```
No `impl` blocks. Methods are defined through `kind` conformance. Dispatch checks what kinds a type's values structurally match.

**Pros:** Unified with the planned `kind` system. No method tables — just structural matching.
**Cons:** Requires `kind` to exist first. Can't define methods that aren't part of a protocol. Every method needs a kind — overkill for `Point.distance()`.

## Discussion
**Current state (2026-03-16):** `Expr::Attr` handles dot access. `eval_attr` in the interpreter has hardcoded cases for enum types (variant access) and namespaces (`std`, `std.ops`). There's no general method lookup.

**Dispatch mechanics:** When the interpreter evaluates `x.foo(args)`, it:
1. Evaluates `x` to a `Value`
2. Evaluates `x.foo` via `eval_attr` — currently returns `VariantConstructor`, `Namespace`, or `BuiltinFn`
3. Calls the result with `args`

Adding method dispatch means step 2 needs: "if `x` has type `T`, and `T` has a method `foo`, return a bound method (or closure over `self`)." This requires:
- A method table: `TypeId → HashMap<String, Value::Func>`
- A way to bind `self`: either a `Value::BoundMethod { receiver, func }` variant, or inject `self` at call time

**`self` semantics:** In a dynamically-typed language, `self` is just the first argument. The question is whether it's explicit (`func distance(self, other)`) or implicit (Python `self`, or implicit receiver like Ruby). Explicit `self` is simpler — it's a regular parameter that the caller doesn't pass when using dot syntax.

**`impl` vs inline:** `impl` blocks are more flexible (add methods after the fact, multiple blocks, separate data from behavior). Inline methods (Option B) are simpler but less extensible. For a language that wants abstract type conformance, `impl` is the natural fit — `impl Kind as Type { ... }` declares conformance, a small step from `impl Kind { ... }`.

**UFCS consideration:** Pure UFCS (Option C) is elegant but problematic in a dynamic language — without static types, the interpreter can't pre-resolve which function `x.foo()` refers to. It would need to: get `x`'s runtime type, find all functions named `foo` in scope, check if any accept that type as first arg. This is slow and fragile. UFCS as a fallback (Option D) is more tractable.

**Prim methods:** Eventually `"hello".len()` or `42.to_str()` should work. These could be:
1. Hardcoded in `eval_attr` (current approach for namespaces) — doesn't scale
2. Virtual `impl` blocks registered at interpreter startup — treat prims like any other type
3. UFCS with stdlib functions — `func len(s: Str): Int { ... }` enables `s.len()`

Option 2 is cleanest — the interpreter registers methods for prim types the same way user code registers them for user types.

**Method table location:** Where does the method table live?
- In `TypeRegistry` — natural home, but currently `TypeDef` is pure data. Adding methods means `TypeDef` holds `Value`s, which creates a dependency cycle (`Value` → `TypeId` → `TypeDef` → `Value`).
- In `Interpreter` — a separate `HashMap<TypeId, HashMap<String, Value>>`. Avoids the cycle. Methods are runtime state, not type definitions.
- The interpreter approach is simpler and avoids coupling type definitions to runtime values.

**Open sub-questions:**
- Static methods? `Point.origin()` vs `origin()` — syntactic preference only in a dynamic language.
- Constructor methods? `Point.new(1.0, 2.0)` vs `Point { x: 1.0, y: 2.0 }` — construction is separate from methods.
- Method resolution order for `kind` conformance — defer to the `kind` proposal.
- Can methods be reassigned? Probably not — methods are bound at definition time, not mutable slots.

## Decision
<!-- blank while open -->

## References
- [prop: type-definitions](type-definitions.md) — product types that methods attach to
- [prop: type-system](type-system.md) — `type` for abstract interfaces, `kind` for concrete product types
- [prop: operator-overloading](operator-overloading.md) — operator dispatch as a special case of method dispatch
- [prop: iteration](iteration.md) — iteration protocol needs `.iter()`, `.next()`
- [phil: stdlib](../phil/stdlib.md) — prim method ergonomics (`.unwrap()`, `.len()`)
- Rust `impl` blocks + traits
- Python `self` parameter convention
- D/Nim UFCS — any `f(x, y)` callable as `x.f(y)`
- Lua metatables — `__index` for method lookup
