# Decision: type system architecture
**ID:** type-system
**Status:** decided
**Date opened:** 2026-03-14
**Date done:** 2026-03-17
**Affects:** lexer, parser, eval, syntax, stdlib

## Question
What is the type architecture for KataScript?

## Context
KataScript currently has a flat `Value` enum (`Nil`, `Bool`, `Num(f64)`, `Str(String)`) in `eval.rs`. Phase 2 adds functions, variables, and control flow; Phase 3 adds user-defined types. The type system needs a foundation that scales from "dynamic scripting language" to "language with structural typing and protocols" without a rewrite at each phase boundary.

The current `Num(f64)` conflates integers and floats, which blocks precise integer arithmetic and fixed-width numerics. This decision defines what types exist, how they're organized, and how they interact.

## Alternatives

### Option A: Flat type set
All types live in one `Value` enum, added as needed. No architectural distinction between "primitive" and "library" types.
**Pros:** Simple; no layering to explain; easy to implement incrementally.
**Cons:** Doesn't scale — adding `List`, `Map`, generics, and user types to one enum gets unwieldy. No path to self-hosting standard types.

### Option B: Two-layer architecture (prim + builtin)
Split types into two layers:
- **Prim** — runtime-handled, irreducible. The evaluator knows these intimately. They cannot be defined in KataScript.
- **Builtin** — shipped with the language but live in global scope. Eventually self-hostable in Phase 4 when the language is powerful enough.

**Pros:** Clean separation of concerns; builtin types can evolve independently; provides a migration path to self-hosting; prim layer stays small and fast.
**Cons:** Two layers to explain; boundary decisions (is `Range` prim or builtin?) require judgment.

### Option C: Everything is an object
All values are objects with methods, including primitives (like Ruby/Smalltalk). No special primitive layer.
**Pros:** Uniform; everything responds to messages; elegant.
**Cons:** Performance overhead for numerics; complex to implement in a tree-walk interpreter; overkill for a scripting language at this stage.

## Design: Two-Layer Type System (Option B)

### Layer 1: Prim types

Runtime-handled, irreducible. The evaluator matches on these directly.

**Numerics:**
- `Int` — arbitrary precision (BigInt)
- Fixed-width signed: `I8`, `I16`, `I32`, `I64`, `I128`
- Fixed-width unsigned: `U8`, `U16`, `U32`, `U64`, `U128`
- Pointer-sized: `Usz`, `Isz`
- `Float` — f64 — IEEE 754 double precision. Arbitrary-precision Float is deferred indefinitely (no current proposal).
- Fixed-width floats: `F16`, `F32`, `F64`

**String/byte:**
- `Str` — immutable string. Internal encoding is NOT guaranteed to callers.
- `Bin` — interned binary blob (Arc<[u8]>, pointer-equality fast path), exact byte sequences.
- `Byte` — single unsigned 8-bit value (bits, not a number — no arithmetic).
- `Char` — Unicode scalar value (codepoint).
- Bridge: `Str.to_bin() -> Bin` (via `ToBin` protocol)

**Other:**
- `Nil` — unit/absence
- `Bool` — `true`/`false`
- `Func` — first-class function value
- `Type` — types are first-class values
- `RawPtr` — opaque handle to runtime-managed storage (cannot be forged from KS)

### Layer 2: Builtin types

Shipped with the language, live in global scope. Written in KataScript itself — the runtime provides only intrinsics that can't be expressed in KS. See `docs/phil/stdlib.md`.

Implemented:
- `Opt[T]` — explicit presence/absence (`std/core/opt.ks`)
- `Res[T, E]` — success/failure (`std/core/res.ks`)
- `Arr[T]` — ordered, growable sequence (`std/dsa/arr.ks`)
- `Map[K, V]` — key-value mapping via open addressing (`std/dsa/map.ks`)
- `Ptr[T]`, `Buf[T]` — typed pointer / typed buffer over RawPtr (`std/mem/`)
- `Allocator` — abstract interface for memory allocation (`std/mem/allocator.ks`)
- Iterator protocols: `Iter[T]`, `ToIter[T]` (`std/core/iter.ks`)
- Lifecycle protocols: `Drop`, `Copy`, `Dupe` (`std/core/lifecycle.ks`)
- Indexing protocols: `GetItem`, `SetItem` (`std/core/indexing.ks`)
- Conversion protocols: `ToBin` (`std/core/conv.ks`)

Not yet implemented:
- `Set[T]` — unordered unique collection
- `Range` — numeric range (for iteration, slicing)

### Type taxonomy (Phase 2–3)

Three keywords for defining types:
- `kind` — concrete product type with named fields ("a kind of thing")
- `enum` — concrete sum type with variants
- `type` — abstract interface/protocol ("the Platonic form"); specifies required methods

Conformance is declared via `impl`:
- `impl Kind { ... }` — attach methods to a kind or enum
- `impl Kind as Type { ... }` — declare that Kind conforms to an abstract Type

This fits the 4-char keyword family: `func`/`kind`/`enum`/`type`/`impl`/`with`.

`kind`, `enum`, `type` (abstract interfaces), and `impl` (including `impl K as T` conformance and the `@` binding sigil for generic vs specialized impls) are all implemented.

### Naming conventions

- Types: `PascalCase` (`Int`, `List`, `MyType`)
- Type parameters: single uppercase letter (`T`, `K`, `V`, `E`)
- Variables and functions: `snake_case`
- Constants: `UPPER_SNAKE`
- Modules: `snake_case`

### Lexer note

Type names (`Int`, `Str`, `List`, etc.) remain `Ident` tokens in the lexer. This is correct for a dynamically-typed language — the parser and evaluator resolve type semantics, not the lexer. No new token variants needed for types.

## Discussion
**Current state (2026-03-14):** `eval.rs` has `Value::Nil`, `Value::Bool(bool)`, `Value::Num(f64)`, `Value::Str(String)`. The `Num(f64)` variant is the main thing that needs splitting — it currently prints integers by checking `n.fract() == 0.0`, which is a hack that breaks at large values.

The two-layer split maps cleanly onto implementation phases:
- Phase 2: expand prim numerics (`Int` replaces `Num`, add `F64`), add `Func`
- Phase 3: add builtin types, platonic typing via `kind`
- Phase 4: self-host builtins in KataScript

Fixed-width numerics (`U8`..`U128`, `I8`..`I128`, `Usz`, `Isz`, `F16`, `F32`, `F64`) are implemented as direct `Value` enum variants. `I256`, `U256`, and `F128` were considered but are not implemented — there is no current proposal.

Arbitrary-precision `Float` is deferred indefinitely. `f64` covers 99% of float needs and there is no current proposal for an arbitrary-precision alternative.

The `Str`/`Bin` split avoids the Python 2 unicode disaster. `Str` is text (encoding opaque), `Bin` is bytes. The bridge is explicit.

Open sub-questions:
- Exact coercion rules between numeric types (implicit widening? explicit only?)
- Whether `Option` and `nil` coexist (see nil-option decision)
- Whether `Result` or exceptions handle errors (see error-handling decision)
- Exact method set on prim types (e.g., does `Int` have `.to_f64()`?)

## Decision
**Chosen: Option B — two-layer architecture (prim + builtin).**

Prim types (`Nil`, `Bool`, `Int`, `Float`, `Str`, `Bin`, `Func`, `Type`, `RawPtr`, `Byte`, `Char`, fixed-width `U8`..`U128`/`I8`..`I128`/`Usz`/`Isz`/`F16`/`F32`/`F64`) are runtime-handled via `Value` enum variants. Builtin types (`Opt[T]`, `Res[T,E]`, `Arr[T]`, `Map[K,V]`, `Ptr[T]`, `Buf[T]`, and the `Iter`/`Drop`/`Copy`/`Dupe`/`GetItem`/`SetItem`/`ToBin` protocols) live in `std/` and are defined using the language's own `enum`/`kind`/`type` keywords. User-defined types use `kind` (product), `enum` (sum), and `type` (abstract interface), with conformance via `impl Kind as Type { ... }`.

`Int` is arbitrary-precision (BigInt). `Float` is f64 (IEEE 754 double); arbitrary-precision Float is deferred indefinitely. Fixed-width numerics (`I8`..`I128`, `U8`..`U128`, `Usz`, `Isz`, `F16`, `F32`, `F64`) are implemented as distinct prim variants. `TypeId` handles (not strings) identify types in a central `TypeRegistry`.

The open sub-questions (coercion rules, nil vs Option, error handling, prim method sets) remain open as separate proposals.

## References
- `katars/src/ks/types.rs` — `TypeRegistry`, `TypeDef`, `TypeId`
- `katars/src/ks/value.rs` — `Value` enum
- [spec: func-vs-fn](func-vs-fn.md) — 4-char keyword family precedent
- [prop: nil-option](../../plan/prop/nil-option.md) — nil vs Option
- [prop: error-handling](../../plan/prop/error-handling.md) — Result vs exceptions
- Python numeric tower (int/float/complex)
- Rust primitive vs std library type distinction
