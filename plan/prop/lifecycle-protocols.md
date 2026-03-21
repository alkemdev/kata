# Decision: lifecycle protocols (Drop, Copy, DeepCopy)
**ID:** lifecycle-protocols
**Status:** open
**Date opened:** 2026-03-21
**Date done:** —
**Affects:** interpreter, types, stdlib

## Question
How should KataScript types control their lifecycle — destruction, shallow copying, and deep copying?

## Context
KataScript currently has no lifecycle hooks. All values are deeply cloned on assignment and parameter passing (`let b = a` clones every field recursively). When a scope is popped, values are simply dropped (Rust `Drop` on the `Value` enum).

This works for simple types but breaks with heap-allocated resources:
- A `Buf[T]` wrapping a `Ptr[T]` needs to deallocate on scope exit (destructor)
- Cloning a `Buf[T]` must allocate new storage and copy elements — not just copy the PtrId handle (deep copy)
- Some types (raw `Ptr[T]`) should never be implicitly copied (move-only)

Three protocols are needed: `Drop`, `Copy`, and `DeepCopy`.

## Alternatives

### Option A: Implicit lifecycle — no protocols
The runtime handles everything. Deep clone is always "clone all fields." Destruction is always "drop the Rust value." No user-defined lifecycle.
**Pros:** Simple. No new concepts. Works for all current types.
**Cons:** Can't express custom destruction (memory leaks for Buf/Ptr). Can't express custom cloning (aliasing bugs for handle-based types). Blocks the memory management architecture.

### Option B: Protocol interfaces — `type Drop`, `type Copy`, `type DeepCopy`
Lifecycle behaviors are defined as `type` interfaces. Types opt in via `impl K as Drop { ... }`. The runtime checks conformance and dispatches automatically.
**Pros:** Consistent with existing interface system. Composable. User-defined types participate equally. Extensible.
**Cons:** Runtime overhead for lifecycle checks. Requires `Self` type for Copy/DeepCopy return types. Interpreter must intercept scope exit and assignment to dispatch protocols.

### Option C: Magic methods — `func __drop(self)`, `func __copy(self)`
Convention-based: if a type has a method named `__drop`, the runtime calls it on scope exit. Like Python's `__del__`, `__copy__`, `__deepcopy__`.
**Pros:** Simple to implement — just check for method existence. No new type system concepts.
**Cons:** Stringly-typed. No compile-time validation. Easy to misspell. Doesn't compose with the interface system.

## Discussion

### Drop

When a value goes out of scope, the runtime must call its `drop` method (if it implements `Drop`) before discarding it. This means:

1. `pop_scope()` iterates all values in the frame being popped
2. For each value, check if its type implements `Drop`
3. If so, call `drop(self)` on it
4. Then discard the value

This adds overhead to every scope exit. Optimization: track which TypeIds have Drop implementations in a `HashSet<TypeId>`. Only check conformance for types in the set.

Nested destruction: if a `kind` has a field that implements `Drop`, should the runtime auto-drop fields? Or must the outer type's `drop` explicitly drop its fields?

**Recommendation:** Auto-drop fields. When popping scope, the runtime recursively walks the value and drops any sub-values that implement Drop. The outer type's `drop` runs first (can clean up), then fields are dropped. This mirrors Rust's drop order.

### Copy

`Copy` marks types that can be cheaply duplicated by copying all fields. For these types, assignment (`let b = a`) just copies. `Copy` types must not own heap resources — all fields must themselves be `Copy`.

Prim types (Int, Bool, Float, Nil) are implicitly `Copy`. User-defined types opt in:

```ks
kind Point { x: Int, y: Int }
impl Point as Copy {}  # all fields are Copy, so this is valid
```

The `impl K as Copy {}` body is empty — the runtime generates the copy. The conformance check validates that all fields are `Copy` types.

### DeepCopy

`DeepCopy` is for types that own resources and need custom duplication logic. When a value is assigned or passed to a function, if its type implements `DeepCopy`, the runtime calls `deep_copy(self)` instead of field-by-field cloning.

```ks
type DeepCopy {
    func deep_copy(self): Self
}
```

This requires `Self` as a type — a placeholder that resolves to the implementing type. `Self` is needed in the method signature to express "returns the same type."

### The `Self` type

`Self` is a pseudo-type available inside `impl` blocks. It resolves to the type being implemented. This is a prerequisite for Copy/DeepCopy.

```ks
impl Buf as DeepCopy {
    func deep_copy(self): Self {
        # Self = Buf[T] here
        ...
    }
}
```

Implementation: when resolving type annotations inside an `impl` block, `Self` maps to the TypeId of the impl target. This is a single special case in `resolve_type_ann`.

### Default behavior

What happens when a type implements neither Copy nor DeepCopy?

Options:
1. **Error on assignment** — types must explicitly declare how they're copied. Strict but annoying.
2. **Field-by-field clone (current behavior)** — the default. Each field is copied according to its own Copy/DeepCopy impl. If a field is neither, it's recursively field-cloned.
3. **Auto-derive DeepCopy** — the runtime generates a field-by-field deep copy. Types that need custom logic override it.

**Recommendation:** Option 3. Auto-derived DeepCopy is the current behavior, just named. Types with handle-based fields (Ptr) MUST override DeepCopy to allocate new storage. The runtime can warn or error if a type contains a Ptr field but doesn't implement DeepCopy.

### Interaction with `unsafe`

`Ptr[T]` should NOT be implicitly copyable. Copying a Ptr aliases the allocation. `Ptr` should be move-only by default — assigning or passing a Ptr moves it, invalidating the source. This is the only type with move semantics.

Move semantics require tracking whether a variable has been moved. This is complex in a dynamic language. Alternative: Ptr is never stored in variables directly — it's always inside a Buf, and Buf implements DeepCopy.

### Protocol dispatch sites

| Event | Protocol checked | When |
|-------|-----------------|------|
| Scope exit | Drop | `pop_scope()` iterates frame values |
| `let b = a` | Copy or DeepCopy | Before binding in new scope |
| Function arg passing | Copy or DeepCopy | When binding params in `call_func_body` |
| `ret val` | Copy or DeepCopy | When returning from function |
| Method copy-out | DeepCopy | When stashing `last_method_self` |

### Prerequisites

- **`Self` type** — needed for DeepCopy return type
- **Generic methods** — `impl Buf[T] as Drop` needs generic impl targets
- **Method lookup fallback** — Drop/DeepCopy on `Buf[Int]` must find impl on `Buf`

### Open questions

- Should `Drop` be called on function return values that are unused? (`divide(10, 2)` as a statement — the Res value is dropped immediately.)
- Should there be a `func finalize(self)` that runs after drop, for guaranteed cleanup? Or is single-pass drop sufficient?
- Cyclic references: if A holds B and B holds A, drop order is undefined. Is this a problem we need to solve, or is it an unsafe footgun?
- Should `Copy` be an empty impl (runtime-generated) or require a `func copy(self): Self` body?

## Decision
<!-- blank while open -->

## References
- [spec: type-system](../../docs/spec/type-system.md) — type keyword for interfaces
- [prop: memory-management](memory-management.md) — Ptr/Buf/Arr architecture
- Rust: `Drop`, `Copy`, `Clone` traits
- Python: `__del__`, `__copy__`, `__deepcopy__`
- C++: Rule of Three/Five (destructor, copy constructor, copy assignment)
- Swift: `deinit`, value types vs reference types
