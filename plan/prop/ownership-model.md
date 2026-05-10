# Decision: ownership model — closures vs. Drop
**ID:** ownership-model
**Status:** open
**Date opened:** 2026-05-09
**Date done:** —
**Affects:** interpreter, scope, value, types, runtime, stdlib

## Question
KataScript bindings are *shared* (closures capture by `Arc<Mutex<Value>>` slot) but Drop is dispatched as if they were *single-owner* (popping a frame finalizes every binding's value, regardless of how many other references exist). What ownership and finalization model should KataScript adopt to make these stories consistent?

## Context

The `e80732c` slot refactor fixed two real bugs (recursion, closure-mutation) by giving every binding a heap cell — `Slot = Arc<Mutex<Value>>` (`katars/src/ks/scope.rs:32`) — that closures capture and write through. Mutation through one path is visible to all paths. Closures, recursion, mutual recursion, and the closure-factory pattern all flow naturally from that one piece of machinery.

But the Drop model does not. In `Interpreter::pop_scope` (`katars/src/ks/interpreter/mod.rs:436`):

```rust
fn pop_scope(&mut self, out: &mut impl Write) {
    debug_assert!(self.call_stack.len() > 1, "cannot pop the global frame");
    let frame = self.call_stack.pop().unwrap();
    if !self.dropping {
        for (_name, value) in frame.drain_lifo() {
            self.drop_value(value, out);
        }
    }
}
```

`Frame::drain_lifo` (`scope.rs:107`) reads each `Slot::get()`, returning a *cloned* `Value`. `drop_value` (`mod.rs:449`) calls the type's `drop` method on that clone. This finalizes the binding without consulting `Arc::strong_count` on the slot. Two pathologies follow.

### Pathology 1: use-after-finalize

`tests/ks/func/closure_drop_capture.ks` (locked-in behavior):

```ks
kind R { id: Int }
impl R as Drop { func drop(self) { print("dropped {self.id}") } }

func make() {
    let r = R { id: 7 }
    func use_r() { print("inside: {r.id}") }
    ret use_r
}

let f = make()
f()
```

Output today:

```
before make
dropped 7        # <- make() returns; r's slot dropped even though use_r captured it
after make
inside: 7        # <- closure body still reads r through the captured slot
before reassign
after reassign
```

The closure resolves `r` correctly because the slot is still alive (`use_r`'s `closure_scope` Arc keeps it alive). But the user's `drop` body has *already run*. If `R::drop` had freed a `Buf`'s memory, the next `f()` call would observe a dangling field. The current behavior is logically incoherent: the value that just had its destructor run is still observable.

### Pathology 2: silent-finalization-loss

When `f` is reassigned to `nil`, the closure dies — and with it the last `Arc<Slot>` reference to `r`'s heap cell. Rust's `Drop` for `Slot` reclaims the underlying `Mutex<Value>`, which deallocates the inner `Value::Rec` and any `Arc`-wrapped substructure — but no KataScript `drop` method fires. Notice the absence of a second `dropped 7` in the expected output.

So: `R::drop` runs *too early* for the visible reference (pathology 1), and *not at all* for the eventual death of the captured reference (pathology 2). The semantics for a value is *"drop fires exactly once when no more references exist,"* but the implementation gives *"drop fires once when the binding's defining scope ends, then the value silently leaks if any closure still holds it."*

### Why this is forced now

- **Lifecycle types are landing.** `Buf[T]` (`std/mem/buf.ks`), `Arr[T]` (`std/dsa/arr.ks`), and any user resource type rely on `Drop` for soundness. As KataScript gains real I/O wrappers and external handles, "drop fires at the wrong time" turns from a curiosity into a soundness break.
- **The closure spec just landed** (`docs/spec/closures.md`, 2026-05-09) and explicitly defers the question. The "Trade-offs" section notes the tension but doesn't resolve it: *"anything captured against the old slot keeps it alive via Arc until the last captor releases it"* — true, but the spec is silent on when (or whether) Drop fires for that final death.
- **The LIFO commit** (`ba88113`, "Drop fires LIFO on scope exit") settled the *order* of finalization within a frame, but the fundamental question — *who owns the value, the slot or the binding?* — was deferred. This proposal closes that loop.

The choice ripples through every droppable type, the closure model, and any future move semantics. It should be made carefully and locked into a spec so future work doesn't drift.

## Alternatives

Each alternative is presented with: mechanism, closure semantics, drop ordering, cyclic-reference handling, migration cost, and substantive pros/cons.

### Option A: Refcount-aware Drop with finalization queue

Keep slots; keep capture-by-shared-reference; teach `pop_scope` to consult `Arc::strong_count` and defer Drop until the slot is genuinely the last reference.

**Mechanism.** `Slot` becomes:

```rust
pub struct Slot {
    cell: Arc<Mutex<Value>>,
    /// Drop method (cached) for the value's type, or None.
    drop_id: Option<MethodId>,
}
```

`pop_scope` now distinguishes "release the binding" from "finalize the value":

```rust
fn pop_scope(&mut self, out: &mut impl Write) {
    let frame = self.call_stack.pop().unwrap();
    for (_, slot) in frame.into_slots_lifo() {
        match Arc::try_unwrap(slot.cell) {
            Ok(mutex) => {
                // Last reference. Finalize now.
                self.drop_value(mutex.into_inner().unwrap(), out);
            }
            Err(arc) => {
                // Still referenced (closure, etc.). Attach a finalization
                // hook so Drop fires when the last Arc dies.
                self.pending_drops.push(PendingDrop { cell: arc, drop_id: slot.drop_id });
            }
        }
    }
    self.flush_pending_drops(out);
}
```

`pending_drops: Vec<PendingDrop>` is checked at safe points — after every statement at the top level, after every `pop_scope` (recursively), after every native call. A `PendingDrop` whose `Arc::strong_count == 1` (i.e., we hold the only reference now) is finalized.

A simpler variant: wrap the cell in a Rust struct with a `Drop` impl that pushes onto a thread-local queue (then the queue is drained at safe points). This avoids the recursion/refcount-watching code at the cost of cross-cutting state.

**Closure semantics.** Unchanged. `let count = 0; func bump() { count = count + 1 }; bump()` works exactly as today: `bump` captures the slot, mutates through it; the binding lives as long as either the defining scope or any captor holds it. The new behavior only kicks in on the *last* release: that's when `R::drop` fires.

**Drop ordering.**
- Within a frame: LIFO over slots that are *uniquely owned* at scope exit (matches `ba88113`).
- For a slot still shared at scope exit: drop fires when the queue observes `strong_count == 1`. This is *non-deterministic order across frames*: if two frames each leak a slot to the same closure, which drops first depends on closure-death order.
- After every reassignment with `=`: existing behavior — drop the *old* `Value` (not the slot). This is unchanged because reassignment writes through the existing slot; only the contents change.

**Cyclic-reference handling.** Cycles leak. `Arc` provides no cycle collection. If a closure captures a slot whose value contains another closure capturing the first slot, neither's `strong_count` ever reaches 1. Mitigations:
- Document the leak. Most KS programs don't construct cycles deliberately.
- Add a `Weak[T]` value variant later (out of scope here).
- Stronger: a periodic mark-and-sweep over `Slot` cells. Heavy machinery; defer.

**Migration cost.** ~50–150 LOC, mostly mechanical:
- `scope.rs`: extend `Slot` with `drop_id` cache; add `Frame::into_slots_lifo() -> impl Iterator<(String, Slot)>` (currently we drain `(String, Value)`).
- `interpreter/mod.rs`: add `pending_drops` field, `flush_pending_drops` method; rewrite `pop_scope`. Hook `flush_pending_drops` into `exec_top_level`, `exec_block` (after pop), `call_func_body` (after restore), `dispatch_loop_flow`.
- `interpreter/stmt.rs`: `Stmt::Assign` already handles its own old-value drop — no change.
- `interpreter/call.rs`: same Slot-aware change to the callee-frame drain.

**Spec impact.** `closures.md` "Drop dispatch on shadowed slots" trade-off section gets an addendum explaining deferred finalization; new spec entry for ownership; one sentence in `lifecycle-protocols.md` updating the dispatch sites table to say "Drop fires when the last reference dies, not at scope exit."

**Test impact.** `closure_drop_capture.expected` rewrites:

```
before make
after make
inside: 7
dropped 7        # now fires when the closure dies
before reassign
dropped 7        # actually... wait. See discussion.
after reassign
```

Subtle: when the user reassigns `f = nil`, the *binding's* slot is replaced by a new `Slot::new(nil)` (because `Frame::set` shadows). The old slot dies. With it, `r`'s slot's last reference dies. So the second `dropped 7` should appear. **This means the proposal as written has the model fire Drop on the closure's death, but each closure capture is one death — so the count of `dropped` outputs depends on capture topology.** That's a fact the user needs to face.

Also new: `tests/ks/lifecycle/drop_after_closure_dies.ks`, `drop_two_captors.ks`, `drop_capture_then_reassign.ks` to lock in the new semantics.

**Pros.**
- Small. Builds on existing slot model. Closure ergonomics unchanged.
- Fixes pathology 1 (no use-after-finalize) and pathology 2 (no silent finalization loss).
- KS user code that *doesn't* rely on shared captures sees identical behavior.
- The "Drop fires when the last reference dies" rule is exactly the natural rule for refcounted languages (Swift's ARC, Python's CPython). Easy to teach.

**Cons.**
- Cycles leak silently. (Same as Swift, same as CPython — though CPython has a cycle collector.)
- Drop ordering across frames is non-deterministic when shared captures are involved. The user has to know that "Drop fires when the last reference dies" doesn't mean "in scope-exit order."
- The pending-drop queue introduces a latent re-entrancy hazard: a user `drop` body that creates new droppable values that themselves get queued must be processed without deadlocking on the queue. Solvable, but new code to write right.
- The drop_id cache duplicates state already in `drop_types: HashSet<TypeId>` on `Interpreter`. Need to pick one source of truth.

### Option B: Refcount-tracked heap values (deep refactor)

Drop the `Slot` abstraction. Every value (or every droppable value) lives in an `Arc<HeapCell>` with a Rust `Drop` impl that fires KataScript's `drop` method.

**Mechanism.** A new wrapper type:

```rust
pub struct HeapCell {
    value: UnsafeCell<Value>,        // mutex-free; we're single-threaded after entry
    drop_hook: Option<DropHook>,     // closure: dispatches KS drop via a back-channel to Interpreter
}

impl Drop for HeapCell {
    fn drop(&mut self) {
        if let Some(hook) = self.drop_hook.take() {
            hook.fire(unsafe { &mut *self.value.get() });
        }
    }
}

pub type ValueHandle = Arc<HeapCell>;
```

`Frame.bindings: IndexMap<String, ValueHandle>`. `let` creates a new handle; `=` writes through to the handle's cell; closures clone handles. `Slot` is *gone* — the handle does both jobs.

The `drop_hook` is the trickiest part: it needs access to the interpreter's `methods` table, `call_func_body`, and the output stream. Two ways:
- **Send-via-queue.** The hook pushes `(value, type_id)` onto a finalization queue. Interpreter drains the queue at safe points. Same re-entrancy concerns as Option A.
- **Direct call via thread-local.** A `thread_local!` `RefCell<*mut Interpreter>` points at the active interpreter; the hook re-enters `call_method_by_id`. Tighter, more direct, but requires care around re-entry during exec.

**Closure semantics.** Behaviorally identical to today: closures capture handles, mutate through them. The `bump` example still works. But now "the slot" is the heap cell: there's one fewer indirection and the abstraction is conceptually cleaner. The "shadowing across capture is invisible" trade-off in the closures spec still holds (a new `let` binds a *new* handle).

**Drop ordering.**
- LIFO within a frame for handles whose refcount drops to 1 at scope exit.
- For shared handles: drops when Rust drops the last `Arc<HeapCell>`. This includes mid-expression handle deaths (e.g., a closure that goes out of scope as part of evaluating the next statement triggers drop *before* the next statement's effects).

**Cyclic-reference handling.** Same as Option A: cycles leak. Same mitigations.

**Migration cost.** ~500–1000 LOC. This is a structural change.
- `scope.rs`: full rewrite. `Slot` deleted. `Frame.bindings: IndexMap<String, ValueHandle>`. `Frame::set/write/get/get_slot` all change signature.
- `value.rs`: `FuncData.closure_scope` still holds a frame chain, but the chain holds handles, not slots.
- `interpreter/mod.rs`: rewrite `get`, `update_in_scope`, `capture_scope`, `pop_scope`, `drop_value`. Add the drop-hook plumbing (queue or thread-local).
- `interpreter/stmt.rs`, `call.rs`, `expr.rs`, `access.rs`: every site that called `Slot::set` / `Slot::with_mut` now operates on handles. The semantic surface area is similar but every call site changes.
- `value.rs`: serde derives still work because `closure_scope` is `#[serde(skip)]`.

**Spec impact.** `closures.md` rewrites the Mechanism section; new spec `ownership.md`; `lifecycle-protocols.md` dispatch table updates.

**Test impact.** Same as Option A in terms of new tests; same rewrite of `closure_drop_capture.expected`.

**Pros.**
- Cleaner conceptual model: there's *one* layer of indirection, not two. "Bindings hold handles to heap cells; heap cells contain values; Drop fires via Rust's `Drop`" is one axiom, not two.
- Drop ordering is fully driven by Rust's `Arc` lifecycle. No queue (in the thread-local variant). No re-entrancy worry except the one shared with Option A.
- Future move semantics are easier: a `move` is "transfer the handle," which Rust models trivially.

**Cons.**
- Largest refactor. Touches every mutating call site. Higher risk of subtle regressions.
- The thread-local trick is a sharp edge — must carefully arrange for `Drop` to never run *outside* an interpreter session (e.g., during interpreter teardown when the call stack is being unwound). Otherwise UB or panics.
- The `UnsafeCell` (or `RefCell` if `Send + Sync` is dropped — see below) makes the borrow rules less obvious. Mistakes here could cause aliasing bugs that don't surface in tests.
- The TUI completer's `Send + Sync` requirement may force `Mutex` over `UnsafeCell` anyway, in which case Option B is no faster than Option A and only differs in structure.

### Option C: Linear ownership + explicit aliasing (Rust-flavored)

Re-architect: bindings are *moved* by default; aliasing is explicit. Closures must declare capture mode.

**Mechanism.** Three changes:
- `let b = a` *moves* `a` into `b`. After the move, `a` is poisoned (any read fails with `ErrorKind::MovedValue`). Implementation: scope frames track a "moved" bitset.
- `let b = share(a)` (or a `&` operator) creates an aliased handle. Internally similar to today's slot model, but only for explicitly aliased values.
- `func` declares capture mode in syntax. `func @move(x) () { ... }` consumes `x`; `func @share(count) () { ... }` aliases the slot for `count`. Default to `@move` to align with let-default-move.

**Closure semantics.** `let count = 0; func bump() { count = count + 1 }; bump()` *no longer compiles* — `bump` would need to be `func @share(count) bump() { count = count + 1 }` (or whatever the syntax shakes out to). The closure-factory pattern becomes verbose.

**Drop ordering.**
- Drop fires at the *unique owner's* scope exit. Always deterministic.
- Aliased values still need refcount-aware Drop to avoid use-after-finalize on the aliased slot — so we still need the Option-A machinery for the aliased subset.

**Cyclic-reference handling.** Cycles only form via aliased values. The aliased subset is a smaller surface, but still leaks unless explicitly broken. Linearity prevents the *common* cycle case (a `kind` field pointing back to its container) by default.

**Migration cost.** Enormous — ~2000+ LOC plus syntax additions plus stdlib retyping.
- New parser tokens for capture modes.
- New AST node for "moved" annotation; rebinding propagates moves through let-destructuring.
- Stdlib (`Arr`, `Buf`, `Ptr`) needs to be retyped: every method call passes self by share or move, and the choice has to be explicit at every call site.
- Every existing test that relies on implicit aliasing breaks.
- All the `pending_drops` machinery from Option A still needed for the share-subset.

**Pros.**
- Strongest ownership story. Drop ordering is fully deterministic. Errors at use-after-move are caught at the binding site.
- Aligns with Rust mental model — for users who know Rust, ergonomics improve overall (you stop fighting hidden aliasing).
- Future compile-to-Rust path is more direct: KataScript moves ↔ Rust moves.

**Cons.**
- Re-architects the language. Closure ergonomics break: the `bump` example becomes ceremony-heavy.
- Out of step with "dynamic scripting language" vibe. Python/Ruby/JS users expect implicit aliasing; KataScript would feel like a Rust subset.
- Doesn't *avoid* the refcount machinery — it just narrows when it's needed.
- Spec rewrite is massive: closures, methods, stdlib types all change.

### Option D: Region-based / arena (alternative direction)

Each scope owns an arena; values live in their owning arena; on scope exit the arena drops. Escape analysis reparents escaping values to the caller's arena.

**Mechanism.** Replace Slot/HeapCell with `ArenaIdx` — an opaque index into an arena attached to the scope. Each arena is `Vec<Option<Value>>`. On scope exit, drain the arena and finalize each value.

To handle escape (a closure escaping to the caller), the runtime tracks data dependencies: when a value is returned, copy it (and transitively-reachable values) into the caller's arena. This is escape analysis at runtime; conservatively, "everything in the closure's transitive reach gets reparented."

**Closure semantics.** `let count = 0; func bump() { count = count + 1 }; bump()` works *if* `bump` doesn't escape — the runtime sees both `count` and `bump` are local to the same arena. If `bump` is returned (closure factory pattern), the runtime must reparent both `count`'s slot and `bump`'s function value to the caller's arena. This is non-trivial: the closure's captured scope chain may span many frames, and reparenting must be transitive.

**Drop ordering.** Deterministic *within* an arena (LIFO over the arena's slots). Across arenas: in scope-exit order.

**Cyclic-reference handling.** Cycles within a single arena are fine — the arena drops the whole graph at once. Cycles spanning arenas are pathological, since the arena can't know the inner cycle has external references.

**Migration cost.** Largest of all options — ~1500–3000 LOC plus runtime escape analysis.
- All scope/value plumbing rewrites.
- Escape analysis: a substantial new system.
- Likely a research project. KataScript would feel different (more like a region-typed language a la Cyclone).

**Pros.**
- Allocation locality: arenas can be linear bumps, very fast.
- Frame exit is O(1) for the bookkeeping plus O(n) for finalization — no per-binding refcount work.
- The "drop everything in this arena now" semantics is the cleanest possible model.

**Cons.**
- Escape analysis is genuinely hard to get right at runtime. Conservative reparenting can pessimize trivial cases (a returned int "escapes," but no one cares).
- KS would feel less like a scripting language, more like a research vehicle.
- Outsized cost for the size of the problem we're actually trying to solve.

---

## Discussion

### What "ownership" should mean here

KataScript today has:
- **Aliasing through closures.** A closure captures a slot; both the outer scope and the closure can read/write it. This is not going away — the closure-factory and counter examples are dominant idioms in scripting.
- **Aliasing through method copy-out.** `self` is copied into the method, mutated, copied back. This is *not* aliasing across simultaneous holders — it's a transient borrow that happens to be implemented as a clone-and-replace. Method copy-out is fine as is; this proposal doesn't disturb it.
- **No aliasing through assignment.** `let b = a` clones the value (cheap because the heavy variants are Arc-wrapped). The two slots are independent.

The first item is the source of all the trouble. As long as a slot can be reached from two places (a frame and a closure's scope chain), Drop has to choose: fire when the *binding* dies (Pathology 1's choice) or fire when the *value* dies (refcount-aware). The latter is closer to the user's mental model — "drop runs once, when no one can see this thing anymore."

### The math of finalization

Refcount-aware Drop has the natural property:
```
∀ value v:  drop_count(v) = 1 ∧ drop_fires_iff(v dies)
```

Eager Drop (today) has:
```
∀ value v:  drop_count(v) ≤ k where k = number of binding scopes that owned v at any point
            drop_fires_iff(any binding holding v dies)  ← wrong
```

The eager rule is broken: it fires once per *binding scope exit*, not once per *value death*. A value held by two captures gets its drop run twice (once per binding scope exit, even though both bindings reference the same heap cell), or once and then leaked, depending on which captor's scope exits first.

Today the test fixture happens to fire drop once because there's exactly one binding scope (the outer `make` frame); the closure's frame doesn't drop because `f`'s slot is replaced by `nil`, releasing the closure but never invoking `r.drop` again. Tomorrow, with two captors, the fixture would either double-drop or miss entirely. The pathology compounds with closure topology.

Refcount-aware Drop (Option A or B) restores the invariant. Linear (Option C) achieves it by *banning* the multi-captor case. Region (Option D) achieves it by escape-reparenting, which is just refcounting under a different name once the dynamics get complex.

### Why this isn't a "TODO: revisit"

The locked-in test (`closure_drop_capture.ks`) cements current behavior as a regression target. Every future Drop test, every Buf/Arr lifetime guarantee, depends on the resolved version of this question. Letting it drift means:
- `Arr[T]::Drop` runs at unpredictable times once arrays start being captured by closures (they will — iterators and lazy combinators are next).
- Any user-written resource type (file handle, network handle) is a footgun.
- Future `Weak[T]` design has to work around the broken Drop story, not on top of a sound one.

### Hash/Eq doesn't drag this proposal

Refcount changes affect Drop only. `Value::eq` and `Value::hash` already operate on the inner Value (through `Slot::get()`'s clone), and would do the same with a HeapCell. No interaction with the numeric-equality proposal.

### Performance angle

- Option A: one extra `Arc::strong_count` check per binding at scope exit. Negligible.
- Option B: deletes one indirection (no slot mutex). Net win in the steady state, dominated by the refactor's own cost.
- Option C: removes refcounting from the linear-subset hot path. Probably faster; offset by the runtime move-tracking bookkeeping.
- Option D: arena is the fastest. But also the largest investment.

For a slow-running interpreter targeting language-design exploration, performance is not the deciding factor.

### Testability angle

Option A's pending-drop queue is the testability concern. If a `drop` body creates a new value that's itself queued, the queue must be drained without infinite recursion or order dependencies. The simplest discipline: process the queue post-order (drain into a local Vec, finalize, repeat until empty). With this rule, a drop-creates-droppable test like:

```ks
kind Outer { id: Int }
impl Outer as Drop { func drop(self) { let inner = Outer { id: self.id + 1 }; ... } }
```

terminates after each level finalizes. The `inner` is bound to a local; when its `drop` body's frame pops, *its* finalization queue drains. A locked-in test would catch any recursion bug.

### What the user's mental model becomes

After the change, the rule is:

> A binding is a name pointing at a heap cell. A heap cell holds a value. When the *last* binding (or capture) releases the heap cell, the value's `drop` method runs once. Re-assignment with `=` mutates the cell; it doesn't release it. Shadowing with `let` binds a new cell; the old cell's value drops if nothing else holds it.

This is cleaner than today's rule and matches what users coming from Swift, Python (CPython), or modern C++ already expect.

### Why not "Mutex with weak refs"?

If we add `Weak[T]` to break user-induced cycles, the implementation could lean further on Rust's `Weak<T>`. That's compatible with Option A and Option B, and orthogonal to this proposal. Don't conflate the two — adopt refcount-aware Drop now, and consider weak refs later as a user-level escape hatch.

## Recommendation

**Option A — refcount-aware Drop with finalization queue.**

The argument:

1. **It fixes the actual bug for the smallest possible cost.** Pathology 1 (use-after-finalize) and pathology 2 (silent loss) both go away. ~50–150 LOC, one new field on the interpreter, one new method on the slot, one rewritten `pop_scope`.

2. **It preserves closure ergonomics.** The counter / closure-factory / mutual-recursion examples in `docs/spec/closures.md` continue to work without rewrites. No user code breaks (except the locked-in test, whose expected output changes).

3. **It is the natural rule for the language KataScript already is.** KataScript captures by shared reference; the only reasonable Drop story is "drop at the last reference." Anything else is a contradiction the implementation has to keep papering over.

4. **It does not foreclose future moves.** If the user later wants linear ownership for resource types, Option A is *compatible*: a `move`-only type with refcount=1 enforced at the binding level is a small extension. Going to Option B is also compatible (the slot becomes a thinner wrapper). Going to Option C from A is harder than going to A from today, but C from A is no worse than C from B.

5. **The trade-offs are honest and well-precedented.** Cycles leak, drop ordering across captures is non-deterministic. Both are accepted in Swift, Python (modulo the cycle collector), and modern C++. The user can reason about them.

The argument *against* Options B/C/D is cost-vs-benefit. They each solve the same problem more thoroughly, but the increment in correctness over A is small while the implementation cost is 5–20× larger. Option B is tempting structurally — the slot abstraction is admittedly redundant once HeapCell exists — but the gain is conceptual cleanliness, not new capability. We can refactor *toward* B incrementally if the abstraction gets in the way.

Option C is the only choice that meaningfully changes the language's character. That's a separate decision that should be made on language-design grounds, not because Drop is broken.

## Implementation finding (2026-05-10): cycle blocks Option A in isolation

Attempted Option A as described. The mechanism (refcount-tracked
slots, `SlotInner::drop` queues finalizers, `drain_finalizers` runs
KS Drop) compiles cleanly and passes the simple non-captured cases.
But it **does not actually fire Drop for slots referenced from a
returned closure**, because of a refcount cycle that the existing
hoist-then-capture sequence creates *for every* `Stmt::FuncDef`:

```
FuncData (Arc) ──► closure_scope: Arc<Scope>
                          │
                          ▼
                       Frame
                          │
                          ▼
                  Slot for use_r ──► Value::Func(Arc::clone of FuncData)
                          ↑                                       │
                          └───────────────────────────────────────┘
```

`hoist_funcs` pre-binds a placeholder slot for `use_r`. `capture_scope`
then snapshots the current frame, which includes that slot. `Stmt::FuncDef`
fills the slot with the new `Func`, whose `closure_scope` is the very
snapshot containing it. The `Arc` cycle is closed at this moment.

Consequence: `Slot::Arc::strong_count` for any captured slot never
reaches 1 — even after every external reference (e.g., the user's
`f` slot) is cleared via `f = nil`. Drop never fires. The simple
closure_drop_capture test goes from "fires once at make() return"
(eager-on-clone, current main behavior) to "fires never" — strictly
worse for the visible-side-effect part of Drop.

**Implication for any Option A implementation:** the refcount-aware
queue is necessary but not sufficient. It needs a cycle-breaker.
Three candidates:

1. **`Weak<Slot>` for self-binding.** The hoisted slot enters the
   captured scope's frame as a `Weak<SlotInner>`, not an `Arc<SlotInner>`.
   The function's body upgrades the weak ref on lookup; if the upgrade
   fails, the function has died. Mutual recursion (a captures b, b
   captures a) breaks if either dies first — needs an additional
   pass to identify which captures are "self-only" vs "outer-state".

2. **AST-driven capture filtering.** Walk the function's body before
   capture; only include slots actually referenced by the body. A
   non-self-referential `func use_r() { print(r.id) }` would not
   capture the `use_r` slot, breaking the cycle without needing
   weak refs. Doesn't help if the body genuinely self-recurses.

3. **Periodic cycle collector.** Run a mark-and-sweep over the slot
   graph at safe points. Heaviest mechanism but handles arbitrary
   user-introduced cycles (closures-in-records-in-closures, etc.).

(1) is the smallest fix and aligns with how Swift handles `weak self`.
(2) is the most performant for the common case but has a corner.
(3) is the most general and matches Python.

**Recommendation update**: Option A + cycle-breaker via (1) for
hoist-induced cycles. (2) and (3) deferred. The cycle-breaker is its
own PR (and its own design call) before any of A's implementation
PRs make sense to land.

## Decision
**Decision:** TBD — pending review. Option A remains the recommended
direction but is now gated on resolving the hoist-cycle problem. See
the implementation-finding section above; the cycle-breaker choice
is the next decision after the high-level model is approved.

## Implementation sketch (for the recommended Option A)

Phase 1 prototype, in commit-shaped chunks. Each chunk should be independently runnable through the conformance suite.

### PR 1 — Slot carries drop metadata

- `katars/src/ks/scope.rs`:
  - `Slot { cell: Arc<Mutex<Value>>, drop_id: Option<MethodId> }` — add `drop_id` as a cached lookup result, populated lazily on first use or by the binder.
  - `Slot::value_arc(&self) -> &Arc<Mutex<Value>>` — accessor for the refcount check.
  - `Frame::into_slots_lifo(self) -> impl Iterator<Item = (String, Slot)>` — replaces `drain_lifo` for the new pop path. Keep `drain_lifo` for places that genuinely want values, mark it `#[deprecated]` for migration.

### PR 2 — Pending-drop queue

- `katars/src/ks/interpreter/mod.rs`:
  - Add `pending_drops: Vec<PendingDrop>` field. `struct PendingDrop { cell: Arc<Mutex<Value>>, drop_id: MethodId }`.
  - `flush_pending_drops(&mut self, out: &mut impl Write)` — drains the queue, finalizing any cell with `strong_count == 1`. Loops until the queue is empty *and* no draining produced new entries.
  - Hook the flush into:
    - `exec_top_level` and `exec_repl` — at the bottom of the per-statement loop.
    - `pop_scope` — after the LIFO drain.
    - `call_func_body` — after the callee-frame drain.
    - `dispatch_loop_flow` — after `pop_scope`.

### PR 3 — pop_scope refactor

- `katars/src/ks/interpreter/mod.rs::pop_scope`:
  - Drain the popped frame's slots LIFO via `into_slots_lifo`.
  - For each `(_, slot)`:
    - `Arc::try_unwrap(slot.cell)` — if `Ok(mutex)`, finalize via `drop_value` immediately.
    - If `Err(arc)`, push `PendingDrop { cell: arc, drop_id: slot.drop_id }`.
  - Call `flush_pending_drops` once at the end.
- `katars/src/ks/interpreter/call.rs::call_func_body`: same shape for the callee-frame drain.

### PR 4 — Stmt::Assign drop path

- `katars/src/ks/interpreter/stmt.rs::Stmt::Assign`:
  - Today: `update_in_scope` returns `Some(old_value)` (a clone of the slot's previous contents); we then `drop_value(old_val, out)`.
  - New: same, but the `drop_value` call now sees a clone — the slot still holds the *new* value. This is fine for fire-once-per-assignment semantics (the old logical value is gone). Hook `flush_pending_drops` after the drop.

### PR 5 — Tests

- Update `tests/ks/func/closure_drop_capture.expected` to the new ordering. Document the expected-output diff in the commit message.
- `tests/ks/lifecycle/drop_after_closure_dies.ks` — closure captures a droppable, returned, then explicitly released. Drop fires after release.
- `tests/ks/lifecycle/drop_two_captors.ks` — two closures capture the same droppable. Verify it fires *once*, when the second closure dies.
- `tests/ks/lifecycle/drop_capture_then_reassign.ks` — outer scope reassigns the binding while a closure still captures the old slot. Verify drop ordering.
- `tests/ks/lifecycle/drop_in_drop_body.ks` — `drop` body creates a new droppable that itself goes out of scope. Verify recursion terminates and ordering is post-order.

### PR 6 — Spec sync

- New `docs/spec/ownership.md` (this proposal's resolved form): mechanism, ordering rule, cycle caveat, references to the implementation.
- Update `docs/spec/closures.md` "Drop dispatch on shadowed slots" trade-off to describe the new behavior. Update example outputs.
- Update `plan/prop/lifecycle-protocols.md` (currently `decided` — but the dispatch-sites table is now stale): clarify that "Drop fires when the last reference dies" — and that scope exit is one such trigger, not the only one.

### Deferred to a follow-up proposal

- **`Weak[T]`** — a value type for breaking user-induced cycles. Not needed for soundness; an ergonomic addition.
- **Cycle collector** — periodic mark-and-sweep over slot cells. Only worth doing if real-world programs hit the cycle leak.
- **Move semantics for resource types** — a `move`-keyword or `@move` attribute that asserts refcount=1 at every binding. Prerequisite for `Ptr[T]` to be properly non-aliasable; see the move-only thread in `lifecycle-protocols.md`.

## References

- `katars/src/ks/scope.rs:32` — `Slot` definition (the unit of sharing today).
- `katars/src/ks/scope.rs:107` — `Frame::drain_lifo` (cloning out values for finalization).
- `katars/src/ks/interpreter/mod.rs:436` — `pop_scope` (the locus of pathology 1).
- `katars/src/ks/interpreter/mod.rs:449` — `drop_value` (recursive into Rec/Tup fields).
- `katars/src/ks/interpreter/mod.rs:466` — `update_in_scope` (writes through shared slots).
- `katars/src/ks/interpreter/stmt.rs:126` — `Stmt::FuncDef` capture sequence (hoist → capture → write through slot).
- `katars/src/ks/interpreter/stmt.rs:259` — `hoist_funcs` (forward references via shared slots).
- `katars/src/ks/interpreter/call.rs:417` — `call_func_body` (callee frame drain at line 467).
- `katars/src/ks/value.rs:171` — `FuncData.closure_scope: Option<Arc<Scope>>`.
- `katars/src/ks/interpreter/types_protocol.rs:33` — `Protocol::Drop` and `MethodId` lookup.
- `tests/ks/func/closure_drop_capture.ks` — locked-in pathology.
- `tests/ks/lifecycle/` — current Drop conformance suite.
- `docs/spec/closures.md` — closure spec; "Drop dispatch on shadowed slots" trade-off section.
- `docs/spec/method-dispatch.md` — `self`'s copy-in copy-out (orthogonal but interacting).
- `plan/prop/lifecycle-protocols.md` — the protocols themselves (Drop, Copy, Dupe).
- `plan/prop/memory-management.md` — Buf / Arr / Ptr depend on Drop firing correctly.
- Commit `e80732c` — "fix: shared-slot scoping for recursion + closure mutation" — the slot model was introduced here.
- Commit `ba88113` — "fix: Drop fires LIFO on scope exit" — settled within-frame ordering.
- Swift ARC (refcount-aware destruction; cycles via `weak`/`unowned`).
- CPython refcount + cycle collector (compatible analog).
- Rust `Drop` (deterministic LIFO via the borrow checker; ours is less strict because we have shared mutable bindings).
- Cyclone / region-typed languages (the academic precedent for Option D).
