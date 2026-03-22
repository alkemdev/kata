# Decision: iteration protocol and `for` loops
**ID:** iteration
**Status:** decided
**Date opened:** 2026-03-16
**Date done:** 2026-03-17
**Affects:** lexer, parser, eval, syntax, stdlib

## Question
How should KataScript implement iteration — what is the iterator protocol, what does `for` look like, and how do types opt in?

## Context
KataScript has `while` loops but no `for` loop and no iteration protocol. Implementing `for` revealed a dependency chain:
1. `for` needs something to iterate over (an iterator protocol)
2. An iterator protocol needs method dispatch (`.next()` on an iterator object)
3. Method dispatch needs product types (iterator state is a struct with fields)

This proposal defines the iteration end-state. It depends on [spec: type-definitions](type-definitions.md) for product types and [spec: method-dispatch](method-dispatch.md) for `.next()` dispatch. It informs the design of `Range`, `List`, and other iterable builtin types.

The [type-system proposal](type-system.md) lists `Range` as a builtin type. The [stdlib plan](../phil/stdlib.md) envisions `List`, `Map`, `Set` as KS-defined types. All of these need an iteration story.

## Alternatives

### Option A: External iterator protocol (Rust/Python model)
Types implement an iterator interface. `for` desugars to repeated `.next()` calls.

```ks
// The protocol (eventually a `kind`):
// - .iter() returns an iterator
// - iterator.next() returns Opt[T] — Val(value) or None to stop

for x in range(0, 10) {
    print(x)
}

// desugars to:
with iter = range(0, 10).iter() {
    while true {
        let next = iter.next()
        if next eq Opt.Non { break }
        let x = next  // unwrap somehow
        print(x)
    }
}
```

**Pros:** Lazy — only computes values on demand. Composable — map/filter/take are iterator combinators. Memory-efficient for large sequences. Well-understood pattern (Rust, Python, Java). Clean separation between the collection and the iteration state.
**Cons:** Requires product types (iterator state), method dispatch (`.iter()`, `.next()`), and `Opt[T]` (signaling completion). The desugaring above also needs `break` (not yet implemented). Heavy machinery for a simple loop.

### Option B: Internal iterator protocol (Ruby/Smalltalk model)
Types accept a callback. `for` passes a closure to the collection.

```ks
// The protocol:
// - .each(callback) calls callback with each element

for x in range(0, 10) {
    print(x)
}

// desugars to:
range(0, 10).each(func(x) {
    print(x)
})
```

**Pros:** Simpler — no iterator object, no `Opt` signaling, no state management. Works with closures (which exist). Doesn't need product types. `break`/`continue` can be modeled as special return values or exceptions.
**Cons:** `break`/`continue` are awkward — a closure can't break out of a loop. Stack depth grows with iteration count (without TCO). Less composable — chaining `.map().filter()` is harder. Can't partially iterate (take first N). Not lazy.

### Option C: Built-in for-range, protocol for custom types
`for x in start..end` is built into the interpreter for numeric ranges. Custom iteration uses the external protocol (Option A) but only when the language is ready.

```ks
// built-in — no protocol needed
for x in 0..10 {
    print(x)
}

// custom — needs protocol (later)
for item in my_list {
    // requires .iter()/.next() dispatch
}
```

**Pros:** Unblocks `for` loops now. Numeric ranges are the 90% case. Custom iteration deferred until product types and method dispatch exist. Incremental.
**Cons:** Two iteration mechanisms — built-in range vs protocol. Range syntax (`..`) adds lexer/parser work. If the protocol lands differently than expected, the built-in path might not unify cleanly.

### Option D: `for` as sugar over `while` + explicit counter
No protocol. `for` is pure syntax sugar.

```ks
for x in range(0, 10) {
    print(x)
}

// desugars to:
let x = 0
while x lt 10 {
    print(x)
    x = x + 1
}
```

**Pros:** Zero new infrastructure. Works now (except `for` keyword parsing). Dead simple.
**Cons:** Only works for numeric ranges. No path to iterating over collections. Not extensible. "for" implies generality that this doesn't deliver.

## Discussion
**Current state (2026-03-16):** `while` works. Closures work. `Opt[T]` exists in the prelude. No product types, no method dispatch, no `break`/`continue`. The parser handles `Expr::Attr` and `Expr::Call`, so `x.next()` would parse if methods existed.

### Protocol design

The external iterator (Option A) is the right long-term answer. It's the pattern that scaled best across languages. The question is what to implement now vs later.

**Iterator as an abstract type:**
```ks
type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}
```
This is the principled version using `type` (abstract interface). Before `type` interfaces exist, the protocol is a convention: types that have `.to_iter()` returning an object with `.next()` work with `for`.

**Opt[T] for completion signaling:** `next()` returning `Opt.Val(value)` or `Opt.Non` is clean. `Opt` already exists. This avoids sentinel values, separate `.has_next()` methods, or exceptions.

### `for` syntax

```ks
for x in expr {
    body
}
```

`for` and `in` are new keywords. `expr` evaluates to something iterable. `x` is bound fresh in each iteration (no pre-declaration needed). `for` is an expression — its value is `nil` (or could accumulate into a list with `for ... yield`, deferred).

Should `for` be an expression that produces a value? Rust's `for` evaluates to `()`. Python's `for` is a statement. KataScript is expression-oriented, but the natural value of a `for` loop is unclear — `nil` is the safe default. A `collect` or `yield` form can come later.

### Range type

`Range` is listed as a builtin type in the [type-system proposal](type-system.md). Options:

1. **Function:** `range(start, end)` or `range(start, end, step)` — returns a Range value. Python-style.
2. **Operator:** `start..end` or `start..=end` — produces a Range value. Rust-style.
3. **Both:** `..` syntax sugar for `Range` construction, `range()` as an explicit alternative.

The `..` syntax is concise but adds lexer/parser complexity (must not conflict with future float literals like `1.0..2.0` — is that `1.0 .. 2.0` or `1.0. .2.0`?). A `range()` function is simpler and sufficient for now.

Range implementation:
- If product types exist: `kind Range { start: Int, end: Int, step: Int }` with an iterator method
- If not: `Value::Range { start, end, step }` as a runtime primitive — less principled but unblocks `for`

### `break` and `continue`

`for` loops need `break` and `continue`. These are control flow that exits or skips the current iteration. Implementation options:
1. Special `Err` variants that the loop machinery catches
2. A `ControlFlow` enum in the interpreter (not exposed to KS)
3. New AST nodes (`Stmt::Break`, `Stmt::Continue`) that the interpreter handles

Option 3 is cleanest — `break` and `continue` are statements that only make sense inside loops. The interpreter's `exec_stmt` returns a signal that the loop handler intercepts.

### Incremental path

Given the dependency chain, the likely implementation order is:

1. **Phase 2 (now):** Add `break`/`continue` to `while`. Add `for` keyword to lexer.
2. **After kind definitions:** Add `Range` kind (either as `Value::Range` prim or a product type).
3. **After method-dispatch:** Add `.iter()`/`.next()` convention. Wire `for` desugaring.
4. **After type interfaces:** Formalize `Iter[T]`/`ToIter[T]` as abstract types.

Option C (built-in for-range first, protocol later) is the pragmatic middle ground. A built-in `for x in range(start, end)` can work without method dispatch if the interpreter special-cases `Range` values. When the protocol lands, the special case becomes the general case.

### Comparison with existing operator dispatch

Operators went through a similar evolution: hardcoded prim dispatch now, user-defined dispatch later via `std.ops.def` or `kind`. Iteration can follow the same pattern: hardcoded Range iteration now, protocol-based iteration later.

## Decision
**Chosen: Option A — external iterator protocol.**

`for x in expr { body }` desugars to: call `.to_iter()` on the iterable, then loop calling `.next()` on the iterator. `.next()` returns `Opt[T]` — `Val(value)` continues, `Non` breaks.

The prelude defines abstract interfaces `Iter[T]` and `ToIter[T]`. Types opt in by implementing these via `impl`. `break` and `continue` work inside `for` loops via `Flow::Bail`/`Flow::Cont` signals in the interpreter.

The iterator object lives as a Rust-local variable (not a KS scope variable). Copy-out semantics apply: after each `.next()` call, the mutated iterator self is written back to the local. This enables stateful iterators (e.g., a counter struct whose `next` increments a field).

`Range` type is deferred — no `..` syntax or `range()` builtin yet. Custom iterators work today via `kind` + `impl` (see `tests/ks/iter/custom_iter.ks`).

## References
- `katars/src/ks/interpreter.rs` — `Expr::For` handler (lines ~494-561)
- `std/prelude.ks` — `Iter[T]`, `ToIter[T]` abstract types
- [spec: method-dispatch](method-dispatch.md) — `.to_iter()`, `.next()` dispatch
- [spec: type-system](type-system.md) — `type` for abstract interfaces
- [phil: stdlib](../phil/stdlib.md) — collections need iteration
