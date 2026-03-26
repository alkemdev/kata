# Decision: string methods
**ID:** string-methods
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** interpreter, stdlib

## Question
How should string methods be implemented — native functions, KS methods via impl, or a hybrid?

## Context
Strings have operators (+, ==, <, etc.) via std.ops but zero methods. No `.len()`, `.chars()`, `.split()`, `.contains()`, `.trim()`, `.substr()`. This blocks any real text processing.

Strings are a prim type (`Value::Str(String)`). Methods on prim types require native function handlers — you can't write `impl Str { ... }` in KS because `Str` isn't a `kind` or `enum`.

## Alternatives

### Option A: Native methods via NativeFnRegistry
Register methods like `str_len`, `str_split`, etc. as native functions. Dispatch via method lookup on the `Str` type (same path as struct methods, but backed by native handlers).
**Pros:** Fast, no KS overhead, full access to Rust's String API.
**Cons:** Every method is Rust code. Can't be overridden or extended by users.

### Option B: Wrapper kind `Str` in KS
Define `kind Str { data: RawStr }` where `RawStr` is the prim. Methods in KS.
**Pros:** Extensible, self-hosted, consistent with the stdlib philosophy.
**Cons:** Wrapping overhead, every string operation goes through method dispatch. `"hello"` literals would need auto-wrapping.

### Option C: Hybrid — native methods + KS sugar
Core methods (len, chars, split, contains, trim, replace, starts_with, ends_with, to_int, to_float) are native. Higher-level utilities can be built in KS later.
**Pros:** Best of both — fast core, extensible surface.
**Cons:** Two layers to understand.

## Discussion
Python, Ruby, and JavaScript all have string methods as built-in. Rust has them as trait impls. For a dynamic scripting language, Option C is most practical — native core methods with room to grow.

Key question: how does method dispatch work on prim types today? The interpreter's `resolve_method` checks `self.methods.get(&type_id)`. Prim types don't have entries in the methods table. We'd need to either:
1. Register native methods in the methods table at bootstrap
2. Add a fallback in `resolve_method` for prim types
3. Define a `StringMethods` module and dispatch to it

Minimum viable set: `len`, `chars` (returns iterator), `contains`, `split`, `trim`, `substr`, `starts_with`, `ends_with`, `to_upper`, `to_lower`, `to_int`, `to_float`, `replace`.

## Decision
Option C (hybrid). Native methods registered at bootstrap, same pattern as Byte/Char/Bin methods. The method dispatch path already works for prim types — Byte, Char, and Bin all have native methods registered in `self.methods[TypeId]` at boot time. Str follows the same pattern.

Minimum viable set for initial implementation:
`len`, `contains`, `starts_with`, `ends_with`, `split`, `trim`, `to_upper`, `to_lower`, `to_int`, `to_float`, `replace`, `substr`, `chars`, `bytes`, `to_bin`.

## References
- Python str methods
- Rust &str / String methods
- Current operator dispatch: `katars/src/ks/native.rs`
