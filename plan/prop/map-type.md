# Decision: Map[K, V] type
**ID:** map-type
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** interpreter, stdlib

## Question
How should maps (dictionaries / hash maps) be implemented?

## Context
Arrays exist (Arr[T] via Buf[T] via Ptr[T] via RawPtr). Maps are the other fundamental collection. Every real program needs key-value storage.

GetItem/SetItem protocols already exist — `map[key]` and `map[key] = val` would work automatically if Map implements them.

## Alternatives

### Option A: Fully in KS via std.dsa
`kind Map[K, V] { ... }` using arrays of key-value pairs (or a hash table backed by Buf). Like how Arr[T] is built on Buf[T].
**Pros:** Self-hosted, consistent with stdlib philosophy. Exercises the type system.
**Cons:** Performance — hash tables in KS would be slow. Hashing needs a protocol.

### Option B: Native prim type
`Value::Map(HashMap<Value, Value>)` as a new prim variant. Native methods for get/set/delete/keys/values/iter.
**Pros:** Fast, simple, works immediately.
**Cons:** Breaks the "builtins are self-hostable" philosophy. Special-cased in the interpreter.

### Option C: Native backing + KS wrapper
`Value::RawMap(u32)` as an opaque handle (like RawPtr). KS defines `kind Map[K, V] { raw: RawMap }` with methods that call native intrinsics. Same layered approach as Ptr → Buf → Arr.
**Pros:** Consistent architecture. KS-visible type with native performance.
**Cons:** More plumbing. Requires a Hashable/Eq protocol for keys.

## Discussion
The Arr architecture (RawPtr → Ptr → Buf → Arr) is proven. Map could follow the same pattern: RawMap (native hash table handle) → Map[K, V] (KS wrapper).

Key design question: what can be a key? Options:
1. Any Value (requires Value to impl Hash + Eq — it doesn't currently)
2. Only prim types (Int, Str, Bool) — simpler but limiting
3. Types implementing a `Hash` protocol — principled but needs protocol infrastructure

For iteration: `impl Map[K, V] as ToIter[Pair[K, V]]` — needs a Pair/Entry type.

Map literal syntax? `{ "a": 1, "b": 2 }` conflicts with struct construction and blocks. Alternatives: `Map { "a": 1 }`, `map(["a", 1], ["b", 2])`, or `%{ "a": 1 }`.

## Decision
<!-- blank while open -->

## References
- Arr[T] architecture: `std/dsa/mod.ks`, `std/mem/mod.ks`
- GetItem/SetItem protocols: `std/core/mod.ks`
- Python dict, Rust HashMap, Go map
