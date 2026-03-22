# Decision: nested assignment (`a.b.c = x`, `a[i][j] = x`)
**ID:** nested-assignment
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** interpreter

## Question
How should nested lvalue assignment work?

## Context
Currently `a.b = x` works and `a[i] = x` works, but `a.b.c = x` and `a[i].field = x` and `a.field[i] = x` all fail with "nested attr/index assignment not yet supported". The interpreter requires the target's root to be a bare `Expr::Name` — it can't walk deeper.

Copy-in/copy-out semantics make this tricky: mutating `a.b.c` means cloning `a`, cloning `a.b`, setting `.c`, writing back `b` into `a`, writing back `a` into scope. Each level of nesting adds a clone-and-writeback step.

## Alternatives

### Option A: Recursive lvalue decomposition
Walk the lvalue chain bottom-up: `a.b.c = x` → get `a`, get `a.b`, set `.c` on the clone, write `b` back to `a`, write `a` back to scope.
**Pros:** General, handles arbitrary depth.
**Cons:** O(n) clones for n levels of nesting. Complex interpreter logic.

### Option B: Mutable references
Instead of copy-in/copy-out, allow `&mut` references into nested fields. Write directly.
**Pros:** Efficient, no cloning. Familiar from Rust.
**Cons:** Major language change. Borrow checker territory. Not appropriate for a dynamic scripting language.

### Option C: Accept the limitation
Document that only single-level assignment works. Workaround: destructure, modify, reconstruct.
**Pros:** No implementation effort. Clear semantics.
**Cons:** Ergonomically painful for nested data structures.

## Discussion
Most dynamic languages (Python, JS, Ruby) handle `a.b.c = x` naturally because objects are reference-typed. KataScript's value semantics (copy-in/copy-out) make this genuinely harder.

The recursive approach (Option A) is the pragmatic answer. It's what the user expects to work. The performance cost (extra clones) is acceptable for a tree-walk interpreter — we're not optimizing for speed.

## Decision
<!-- blank while open -->

## References
- Current impl: `exec_attr_assign` and `exec_item_assign` in interpreter.rs
- Copy-in/copy-out: method dispatch in interpreter.rs
