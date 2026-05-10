# Decision: method dispatch
**ID:** method-dispatch
**Status:** decided
**Date opened:** 2026-03-16
**Date done:** 2026-03-17
**Affects:** parser, eval, syntax

## Question
How should KataScript associate functions with types and dispatch `.method()` calls?

## Context
KataScript has dot access (`Expr::Attr`) and function calls (`Expr::Call`), so `x.foo(y)` already parses as "access `foo` on `x`, then call the result with `y`." But there's no mechanism to define what `foo` means on a given type. Currently, dot access is hardcoded for:
- Enum types: `Opt.Val` produces a `VariantConstructor`
- Namespaces: `std.ops` resolves sub-namespaces and builtins

Method dispatch is needed for:
- **Iteration** ([spec: iteration](iteration.md)) — types need `.to_iter()`, `.next()` methods
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
- In `Interpreter` — a separate per-type method table. Avoids the cycle. Methods are runtime state, not type definitions.
- The interpreter approach is simpler and avoids coupling type definitions to runtime values.

**Method-name keying:** What identifies a method in the table?
- Strings (`HashMap<String, Value>`) — direct, but every dispatch hashes a `&str` and pays a comparison on hash collision. The hot path (every `for` body iteration calls `.next()`, every scope exit may call `.drop()`) burns time on identical string compares.
- Interned handles (`HashMap<MethodId, Value>` where `MethodId(u32)` is an index into a names table) — names are hashed once at registration time, lookups are u32-keyed. Source-level method names like `"to_iter"` and `"next"` round-trip through a `MethodInterner` so diagnostics can still recover the human name.
- Handle-based keying is what we want: the registration cost is paid once, and the runtime dispatch path becomes a u32 hash hit with no string work.

**Open sub-questions:**
- Static methods? `Point.origin()` vs `origin()` — syntactic preference only in a dynamic language.
- Constructor methods? `Point.new(1.0, 2.0)` vs `Point { x: 1.0, y: 2.0 }` — construction is separate from methods.
- Method resolution order for `kind` conformance — defer to the `kind` proposal.
- Can methods be reassigned? Probably not — methods are bound at definition time, not mutable slots.

## Decision
**Chosen: Option A — `impl` blocks (Rust model).**

Methods are defined in `impl Type { func method(self, ...) { ... } }` blocks. `self` is an explicit first parameter. Multiple `impl` blocks per type are allowed. Dispatch looks up the method in a per-`TypeId` method table stored in the `Interpreter` (not `TypeRegistry`, avoiding the `Value` ↔ `TypeDef` dependency cycle).

The methods table is keyed by an interned `MethodId(u32)` handle, not by `String`. The shape is `HashMap<TypeId, IndexMap<MethodId, Value>>` (see `katars/src/ks/interpreter/mod.rs:51`). At registration time, each method name is interned through a `MethodInterner` (`katars/src/ks/interpreter/method_id.rs:33`); at lookup time, a `&str` is resolved to a `MethodId` via `MethodInterner::lookup` (read-only — never inserts), and the table is queried by handle. The hot path is `Interpreter::lookup_method_by_id` (`katars/src/ks/interpreter/call.rs:26`), with `lookup_method` (line 48) as the name-keyed convenience wrapper that delegates after one interner lookup.

Mutation uses copy-in copy-out semantics: the interpreter snapshots `self` before the call, executes the body, then writes the final `self` value back to the receiver variable. This only works for simple `var.method()` receivers (not nested attribute chains).

Conformance: `impl Kind as Type { ... }` declares that `Kind` satisfies an abstract `Type` interface. The interpreter checks that all required methods exist with matching parameter counts.

UFCS (Option D) was deferred — it's tractable as a future extension but adds dispatch complexity that isn't needed yet.

## Mechanism: MethodInterner / ProtocolMethods

`MethodInterner` (`katars/src/ks/interpreter/method_id.rs`) is a bidirectional name ⇄ handle table:
- `intern(&mut self, name: &str) -> MethodId` — insert-or-fetch; called from registration paths.
- `lookup(&self, name: &str) -> Option<MethodId>` — read-only; called from the dispatch hot path.
- `name(&self, id: MethodId) -> &str` — recover the source name for diagnostics (e.g., the `NoAttr` error message in `resolve_method_by_id`, `katars/src/ks/interpreter/call.rs:84`).

`ProtocolMethods` (`katars/src/ks/interpreter/method_id.rs:69`) holds the `MethodId`s of the language-level protocol methods (`to_iter`, `next`, `drop`, `get_item`, `set_item`). It is constructed once in `Interpreter::new` (`katars/src/ks/interpreter/mod.rs:130`) by interning each `Protocol::method_name()`, and is the single source of truth from then on. The runtime never re-interns these names; it reaches for the cached handle:

```rust
// for-loop iteration, expr.rs:496
let to_iter_id = self.protocol_methods.to_iter;
let next_id = self.protocol_methods.next;
let iter_val = self.call_method_by_id(&iterable, to_iter_id, &[], out)?;
// ...
let bound = self.resolve_method_by_id(&iterator, next_id)?;
```

```rust
// drop dispatch on scope exit, mod.rs:449
let drop_id = self.protocol_methods.drop;
let _ = self.call_method_by_id(&value, drop_id, &[], out);
```

```rust
// indexed write a[k] = v, access.rs:119
let set_item_id = self.protocol_methods.set_item;
self.call_method_by_id(&receiver, set_item_id, &call_args, out)
```

Each of these sites used to do a `String` (or `&'static str`) lookup against a string-keyed table. They now go through `call_method_by_id` / `resolve_method_by_id`, which take a `MethodId` directly and skip the interner entirely on the hot path.

User-defined methods register through `Interpreter::register_impl_methods` (`katars/src/ks/interpreter/registration.rs:262`), which interns each method name once and inserts the resulting `MethodId → Value::Func` pair into the type's `IndexMap`. Native prim-type methods follow the same pattern via `register_native_methods` (`katars/src/ks/interpreter/mod.rs:206`). `IndexMap` is used (not `HashMap`) so iteration order matches registration order — which is what REPL completion relies on (`collect_methods`, `mod.rs:333`).

## References
- `katars/src/ks/interpreter/method_id.rs` — `MethodId`, `MethodInterner`, `ProtocolMethods`
- `katars/src/ks/interpreter/call.rs` — `lookup_method_by_id`, `resolve_method_by_id`, `call_method_by_id`, `call_func_body` (copy-in copy-out)
- `katars/src/ks/interpreter/registration.rs` — `register_impl_methods` (intern at registration)
- `katars/src/ks/interpreter/types_protocol.rs` — `Protocol` enum + `method_name()`
- [spec: type-system](type-system.md) — `type` for abstract interfaces, `kind` for concrete product types
- [prop: operator-overloading](../../plan/prop/operator-overloading.md) — operator dispatch as a special case of method dispatch
- [spec: iteration](iteration.md) — iteration protocol needs `.to_iter()`, `.next()`
- [phil: stdlib](../phil/stdlib.md) — prim method ergonomics
