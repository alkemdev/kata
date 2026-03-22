# Decision: method dispatch on primitive types
**ID:** prim-methods
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** interpreter

## Question
How should methods on primitive types (Int, Float, Str, Bool) work?

## Context
Method dispatch (`resolve_method`) looks up methods in `self.methods` keyed by TypeId. Prim types have TypeIds (prim::INT, prim::STR, etc.) but no entries in the methods table. You can't write `impl Int { ... }` in KS because `Int` isn't defined as a `kind` or `enum`.

This blocks: string methods (`"hello".len()`), numeric methods (`42.abs()`), and any method-based API on primitives.

## Alternatives

### Option A: Register native methods at bootstrap
In `bootstrap()`, register method entries for prim types in the interpreter's methods table. Each method points to a native function handler.
**Pros:** Uses existing dispatch path. No interpreter changes. Methods appear in the same table as struct methods.
**Cons:** All methods must be native (Rust) code. Can't be overridden or extended in KS.

### Option B: Allow `impl` on prim type names
Parse `impl Int { func abs(self): Int { ... } }` — the interpreter resolves `Int` to `prim::INT` and registers methods normally.
**Pros:** KS-writable methods on prims. Consistent syntax with struct/enum impl.
**Cons:** `self` would be a prim value — copy semantics. Can KS code operate on a bare Int meaningfully? Most useful methods need access to the underlying Rust representation.

### Option C: Hybrid — native core + KS extension
Native methods for operations that need Rust access (str.len, str.split). Allow `impl Int { ... }` for KS-level additions.
**Pros:** Best of both.
**Cons:** Two registration paths.

## Discussion
Option A is simplest and sufficient for now. The string-methods proposal depends on this decision. Int and Float need fewer methods (abs, pow, min, max). Bool probably needs none.

The key implementation question: how does `resolve_method` find methods registered at bootstrap? Currently it only checks `self.methods.get(&type_id)`. If we insert entries for prim types during bootstrap, the existing path works unchanged.

## Decision
<!-- blank while open -->

## References
- Current method dispatch: `interpreter.rs` resolve_method, lookup_method
- Native function system: `native.rs` NativeFnRegistry
- Blocked by: string-methods proposal
