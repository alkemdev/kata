# Decision: closures and slot-based scope capture
**ID:** closures
**Status:** decided
**Date opened:** 2026-05-09
**Date done:** 2026-05-09
**Affects:** interpreter, scope, runtime

## Question
How should KataScript model variable bindings so that:
1. A `func` can call itself by name (recursion).
2. Two `func`s defined as siblings can call each other (mutual recursion / forward reference).
3. A closure that mutates a name from an enclosing scope writes through to that scope (and to other closures sharing it).
4. A factory that returns a closure can give each instance its own private state.

## Context
The closure-and-recursion audit (commit `e80732c`) found two regressions with one shared root cause: `Frame.bindings` stored `Value` directly. Closures took a snapshot of values at capture time, so:
- A top-level `func factorial` couldn't recurse — when the body ran, the captured scope held `Value::Nil` (the placeholder), not the function value that landed in scope a moment later.
- A closure that wrote to an outer name had nothing to write through — `update_in_scope` saw `closure_scope` as a frozen value tree.

Both call for the same fix: bindings need to be shared mutable cells, not values, so that mutations made anywhere (including writes through a captured scope) are visible everywhere the cell is reachable.

## Alternatives

### Option A: Capture by value (snapshot at definition)
The function's `closure_scope` holds a deep copy of every name visible at definition time.

**Pros:** Trivially thread-safe, no aliasing, no synchronization. Closures behave like data.
**Cons:** Recursion needs a special case — the function value isn't a value yet when the body captures. Mutual recursion doesn't work without a separate hoisting pass that produces real values, not placeholders. A closure that "mutates" the outer scope is actually mutating a private copy — surprising. Two closures sharing a counter need a side channel (mutable cell threaded through arguments). Fights every dynamically-typed scripting language's expectations.

### Option B: Capture by immutable reference
Captured names are read-only inside a closure. Writes target only the local frame.

**Pros:** Common in functional languages (e.g., OCaml without `ref`). Simple semantics — captured names are facts about the world at definition time.
**Cons:** Defeats the closure-factory pattern. A `make_counter` returning a closure that bumps a counter is the canonical motivating example, and it doesn't work without explicit cells. Forces every user with shared mutable state to manually wrap values in `Ptr` or `Buf`. Out of step with language status (KS already has assignment as a first-class statement).

### Option C: Capture by shared reference (slot-based)
Every name in every frame holds a `Slot` — a reference-counted, interior-mutable cell. `let` creates a fresh slot. Assignment writes through the existing slot. Closures capture the scope chain by `Arc`-cloning frames; the underlying `Slot` is shared, so mutation through any reachable path is visible to all others.

**Pros:** Natural recursion (the function's name resolves through the same slot the body sees). Natural mutual recursion (paired with hoisting). Closure factory works without ceremony. Closures can mutate outer state, and the mutation is a first-class operation, not a workaround. Lines up with the dynamic-scripting model the rest of the language assumes.
**Cons:** Mutex acquisition cost on every read/write. `Slot::set` shadowing has a subtle interaction with already-captured slots (intentional, but worth documenting).

## Decision
**Chosen: Option C — capture by shared reference (slot-based).**

A binding is a `Slot`, not a `Value`. Frames hold slots; captured scopes hold the same slots. Mutation of a name writes through the slot, so every captor sees the new value.

## Mechanism

### `Slot`: shared interior-mutable cell

`Slot = Arc<Mutex<Value>>` (`katars/src/ks/scope.rs:32`).

The interpreter is single-threaded, so the synchronization is uncontended in practice. `Mutex` (rather than `RefCell`) is required because the TUI completer (`KataCompleter`) holds a reference to the live interpreter and `rustyline` requires `Send + Sync` on its completion handler. `RefCell` would refuse to compile.

`Slot` exposes only three operations on the cell:
- `get()` clones the inner `Value` (cheap — every heavy `Value` variant is itself `Arc`-wrapped).
- `set(v)` atomically replaces the cell's contents and returns the previous value (used for drop dispatch by the caller).
- `with_mut(f)` runs a closure with `&mut Value` (used for in-place struct-field updates without an extra clone).

The mutex guard never escapes a single `get`/`set`/`with_mut` call, so deadlock by re-entry on the same thread is structurally impossible.

### `Frame`: name → slot map

`Frame.bindings: IndexMap<String, Slot>` (`katars/src/ks/scope.rs:59`). `Frame` itself stores no values directly — every binding is a `Slot`. Two `Frame`s that share the same `Slot` for a name see each other's writes.

Two operations matter for closure semantics:
- `Frame::set(name, value)` — `let`-style. **Always creates a new `Slot::new(value)`**, even if the name is already bound. This is shadowing: the previous slot (if any) becomes orphaned in this frame. Any closure that captured a reference to the *previous* slot still points at the old value (see `frame_set_creates_new_slot` in scope.rs tests).
- `Frame::write(name, value)` — assignment-style. Writes through the existing slot if present, returning the old value. Does nothing if the name isn't bound in this frame.

### `Scope`: captured frame chain

`Scope { frame: Frame, parent: Option<Arc<Scope>> }` (`katars/src/ks/scope.rs:126`). A closure's `closure_scope: Option<Arc<Scope>>` is built by `Interpreter::capture_scope` (`katars/src/ks/interpreter/mod.rs:418`) at the moment the function value is constructed. The capture walks the live `call_stack` from the global frame inward, cloning each `Frame` and chaining them as parents. Cloning a `Frame` clones its `IndexMap<String, Slot>` — i.e., the slot `Arc`s, not their contents — so the captured scope shares slots with whatever frames are still live.

`Scope::lookup_slot` (`scope.rs:141`) walks the frame chain returning the first `Slot` that matches a name. This is what `Interpreter::update_in_scope` uses to write through into a captured scope.

### Reading and writing a name

`Interpreter::get` (`mod.rs:374`) reads through call-stack frames innermost first, then falls through to `closure_scope`.

`Interpreter::update_in_scope` (`mod.rs:466`) walks the *same* path for writes:

```rust
for frame in self.call_stack.iter().rev() {
    if let Some(old) = frame.write(name, value.clone()) {
        return Ok(Some(old));
    }
}
if let Some(scope) = self.closure_scope.as_ref() {
    if let Some(slot) = scope.lookup_slot(name) {
        let old = slot.set(value);
        return Ok(Some(old));
    }
}
Err(ErrorKind::Undefined { ... })
```

The closure-scope branch is the load-bearing line: `slot.set(value)` mutates the same `Arc<Mutex<Value>>` that the outer scope still references, so the write propagates.

### Function definition: capture, then write through

`Stmt::FuncDef` handling lives in `katars/src/ks/interpreter/stmt.rs:126`. The sequence is:

1. If no slot exists for the function's name, create one (fallback for blocks where `hoist_funcs` didn't run).
2. `capture_scope()` — freeze the visible scope as `Arc<Scope>`. The function's *own* slot is already in this scope (step 1 / hoisting put it there).
3. Build `Value::Func(Arc::new(FuncData { ..., closure_scope: Some(captured) }))`.
4. `update_in_scope(name, func)` — write the real function value through the existing slot.

Because the captured scope contains the same slot, the function's body resolves the function's own name to the function value once step 4 completes. Recursion works without a special case.

`FuncData.closure_scope: Option<Arc<super::scope::Scope>>` lives at `katars/src/ks/value.rs:171`. It is `#[serde(skip)]` — the scope is runtime state, not part of the AST.

### `hoist_funcs`: forward references and mutual recursion

Before any statement in a block executes, `Interpreter::exec_block` calls `hoist_funcs` (`stmt.rs:246`):

```rust
for stmt in stmts {
    if let Stmt::FuncDef(FuncDef { name, .. }) = &stmt.node {
        if !self.call_stack.last().map_or(false, |f| f.contains(&name.node)) {
            self.set(name.node.clone(), Value::Nil);
        }
    }
}
```

Every `func` in the block gets a placeholder slot before any body runs. By the time any sibling captures the scope, every other sibling's slot already exists — so when each `func` later writes its real `Value::Func` through its slot, captured scopes see the update via the shared `Arc`.

This is what makes mutual recursion work: `is_even` captures the scope while `is_odd` is still `Nil`, but the slot for `is_odd` is already in the captured scope. When `is_odd`'s definition lands a few lines later, its `update_in_scope(name, func)` writes through the same slot, and `is_even`'s body resolves `is_odd` to the real function on the first call.

`hoist_funcs` is a no-op for blocks without `Stmt::FuncDef`. It also runs at the top of `exec_top_level` and `exec_repl` for the same reason.

### Function call: install the captured scope

`Interpreter::call_func` (`katars/src/ks/interpreter/call.rs:454`) saves the caller's `call_stack` and `closure_scope`, replaces them with a fresh `Frame` and the callee's `closure_scope`, executes the body, then restores. While the body runs, name lookup walks the new `call_stack` first (locals + parameters), then the captured `closure_scope`. Slots reached through the captured scope are shared with whatever frames the closure was built against — that's how outer-scope mutation propagates.

## Examples

### Recursion (`tests/ks/func/recursion.ks`)

```ks
func fact(n: Int): Int {
    if n == 0 { ret 1 }
    ret n * fact(n - 1)
}
print(fact(0))
print(fact(5))
```

When the parser hands the `Stmt::FuncDef` to the interpreter, `hoist_funcs` has already created a placeholder slot for `fact` at the top level. `capture_scope` freezes a scope that contains that slot. The slot then receives the real `Value::Func`. Inside the body, `fact(n - 1)` resolves `fact` through the captured scope's slot — which now holds the function — and the recursive call works.

### Mutual recursion (`tests/ks/func/mutual_recursion.ks`)

```ks
func is_even(n: Int): Bool {
    if n == 0 { ret true }
    ret is_odd(n - 1)
}
func is_odd(n: Int): Bool {
    if n == 0 { ret false }
    ret is_even(n - 1)
}
print(is_even(10))   # true
```

Both names are hoisted before either definition executes. `is_even` captures while `is_odd` is still `Nil`; that doesn't matter — the *slot* for `is_odd` is in the captured scope, and `is_odd`'s definition fills it before any call happens.

### Closure mutates outer (`tests/ks/func/closure_mutates_outer.ks`)

```ks
let count = 0
func incr() { count = count + 1 }
incr()
incr()
incr()
print(count)         # 3
```

`incr` captures a scope that contains `count`'s slot. The `count = count + 1` assignment in the body becomes `update_in_scope("count", new_value)`. The interpreter walks the callee's empty local frame first, finds nothing, then looks in `closure_scope`, finds the slot, and writes through it. The outer scope sees the same slot, so `print(count)` reads the new value.

### Closure factory (`tests/ks/func/closure_factory.ks`)

```ks
func make_counter() {
    let count = 0
    func incr(): Int {
        count = count + 1
        ret count
    }
    ret incr
}
let c1 = make_counter()
let c2 = make_counter()
print(c1())  # 1
print(c1())  # 2
print(c2())  # 1
print(c1())  # 3
```

Each call to `make_counter` runs the body in a fresh frame: a fresh slot for `count` is created by `let count = 0`, and the inner `func incr` captures a scope containing that fresh slot. Returning `incr` keeps the captured scope alive (via the `Arc` chain) — but `c1` and `c2` capture *different* slots, because `let` always creates a new one. They have private state.

## Trade-offs

- **Mutex cost on every read/write.** Each `get` and `set` takes the mutex. Uncontended in single-threaded use, but it's still a real atomic operation. We accept the cost because the alternative — `RefCell` — won't compile against the TUI completer's `Send + Sync` bound. Profiling has not shown this to be hot.
- **Shadowing across capture is invisible to the closure.** If outer code does `let x = 1; func f() { ... x ... }; let x = 2`, the second `let` creates a *new* slot. `f`'s captured scope still points at the original slot holding `1`. This is intentional and matches the lexical-shadowing intuition: each `let` introduces a new variable, scoped to whatever block it's in. Reassignment with `=` writes through the existing slot and *is* visible to the closure.
- **`closure_scope` is `#[serde(skip)]`.** Function values can't be round-tripped through the AST dump — the captured scope is runtime state. `--dump-ast` users only see the body's AST, which is correct for that tool's purpose.
- **Drop dispatch on shadowed slots.** When `Frame::set` overwrites a binding, it returns the old value to the caller (`stmt.rs` `Stmt::Assign` handling) which then runs `drop_value`. The shadow case in `set` returns the old `Slot`'s `get()` — fine for drop dispatch, but worth noting that anything captured against the *old* slot keeps it alive via `Arc` until the last captor releases it.

## References
- `katars/src/ks/scope.rs` — `Slot` (`:32`), `Frame` (`:59`), `Scope` (`:126`), `Scope::lookup_slot` (`:141`)
- `katars/src/ks/interpreter/mod.rs` — `capture_scope` (`:418`), `update_in_scope` (`:466`), `get` / `get_slot` (`:374`, `:385`)
- `katars/src/ks/interpreter/stmt.rs` — `hoist_funcs` (`:246`), `Stmt::FuncDef` handling (`:126`)
- `katars/src/ks/interpreter/call.rs` — context switch into `closure_scope` (`:454`)
- `katars/src/ks/value.rs` — `FuncData.closure_scope` (`:171`)
- Conformance tests (`tests/ks/func/`):
  - `recursion.ks` — function calls itself by name
  - `mutual_recursion.ks` — paired `func`s reference each other (forward reference)
  - `closure_mutates_outer.ks` — closure writes through to outer scope
  - `closure_factory.ks` — factory produces closures with private per-instance state
  - `closure_over_outer.ks` — closure reads an outer binding (read-only path)
- Commit `e80732c` — "fix: shared-slot scoping for recursion + closure mutation"
- [spec: method-dispatch](method-dispatch.md) — `self`'s copy-in copy-out interacts with slot-based assignment
