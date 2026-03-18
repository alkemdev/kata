# Decision: operator overloading
**ID:** operator-overloading
**Status:** open
**Date opened:** 2026-03-16
**Date done:** —
**Affects:** interpreter, syntax, stdlib

## Question
How should user-defined types participate in operator dispatch?

## Context
KataScript has a unified operator dispatch system: `a + b` and `std.ops.add(a, b)` are equivalent. Currently only primitive types (Int, Float, Str) have operator implementations, hardcoded in the interpreter. The `std.ops` namespace exposes these as callable functions.

The next step is letting user-defined types (enums, and eventually structs) define their own operator behavior. For example, a `Vec2` type should be able to make `+` work.

Key constraints:
- Prim ops must remain in the runtime (can't implement Int+Int in KS).
- Dispatch currently assumes both operands are the same type. Cross-type dispatch (e.g., `Vec2 + Float`) is a future concern.
- KataScript doesn't have methods, traits/kinds, or pattern matching yet. The mechanism chosen here should be forward-compatible with those features but not depend on them.
- `std.ops.add(a, b)` must remain equivalent to `a + b` for any type — the dispatch path is the same.

## Alternatives

### Option A: Registration function — `std.ops.def(name, type, fn)`
Users call `std.ops.def("add", Vec2, my_fn)` to register a function as the `add` implementation for `Vec2`. Dispatch checks a `(op_name, TypeId) → fn` table before falling back to prim implementations.
**Pros:** Works today — no new syntax, no traits/kinds needed. Simple to implement (just a HashMap in the interpreter). Clear and explicit.
**Cons:** Stringly-typed (`"add"` instead of a symbol). No compile-time or parse-time validation. Global mutable state (the override table). Doesn't compose — what if two libraries both register `add` for the same type?

### Option B: Named method convention — `fn add(self, other)`
If a type has a function named `add` in scope (or attached to the type), operator dispatch finds it by convention. No explicit registration needed.
**Pros:** Familiar (Python `__add__`, Ruby operator methods). Less ceremony than explicit registration. Works with future method syntax.
**Cons:** Requires method dispatch or at minimum a way to associate functions with types. Implicit — harder to see what's happening. Name collisions with regular functions.

### Option C: Abstract type-based — `type Addable { func add(self, other) }`
Operator protocols are defined as abstract types. Concrete kinds/enums conform via `impl Kind as Addable { ... }`. `type Addable` is defined in `std/ops.ks`.
**Pros:** Principled and composable. Static-like guarantees even in a dynamic language. Forward-compatible with structural typing. Rust/Haskell-validated approach.
**Cons:** Requires the `type` interface system to exist first. Heavier machinery. Might be overkill for a scripting language.

### Option D: Hybrid — registration now, abstract types later
Start with Option A (`std.ops.def`) as the low-level mechanism. When `type` interfaces land, `type Addable` becomes sugar that calls `std.ops.def` under the hood. Both paths coexist.
**Pros:** Incremental. Unblocks operator overloading now. Abstract types add structure later without breaking existing code.
**Cons:** Two mechanisms to explain. The registration API might calcify if abstract types take a different shape than expected.

## Discussion
**Current state (2026-03-16):** `std.ops` exists as a builtin namespace. Operators dispatch through `eval_binop` for prims. No override mechanism exists yet. No methods, kinds, or pattern matching.

The registration approach (Option A) is the smallest useful step. It requires:
1. A `HashMap<(String, TypeId), Value>` in the interpreter for overrides.
2. A `std.ops.def` builtin that inserts into it.
3. A check in `eval_binop`: before the prim match, look up `(op_name, left.type_id())` in the override table and call the registered function if found.

Open sub-questions:
- **Dispatch on left type only vs both types?** Left-only (Python model) is simpler. For same-type operations this doesn't matter. Cross-type dispatch (double dispatch, multimethods) is a harder problem — defer it.
- **Should overrides apply to prims?** Could a user redefine `Int + Int`? Probably not — prim behavior should be sealed. But `std.ops.def("add", MyIntWrapper, ...)` should work.
- **Symmetry:** If `Vec2 + Vec2` is defined, should `std.ops.add(a, b)` and `a + b` both find it? Yes — same dispatch path.

## Decision
<!-- blank while open -->

## References
- [spec: type-system](../../docs/spec/type-system.md) — `type` for abstract interfaces
- Python data model: `__add__`, `__radd__` for reverse dispatch
- Rust `std::ops::Add` trait
- Julia multiple dispatch — dispatches on all argument types
