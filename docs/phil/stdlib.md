# Standard Library Plan

## Principle

The KataScript standard library is written in KataScript. The runtime provides only the minimal primitives that *cannot* be expressed in KS (I/O, memory allocation, type introspection). Everything else — `Opt[T]`, `Res[T, E]`, collections, iterators — is plain `.ks` code.

## Directory

```
std/
├── prelude.ks          # auto-loaded: Opt, Res, core utilities
├── collections.ks      # List, Map, Set (when self-hostable)
├── io.ks               # wrappers around runtime I/O primitives
└── math.ks             # numeric utilities, constants
```

`std/prelude.ks` is loaded automatically before user code. Other modules require explicit import.

## What the runtime must provide

These are intrinsics — things KS code cannot define for itself:

| Intrinsic | Why |
|-----------|-----|
| `print(...)` | I/O — needs access to the output writer |
| `type_of(v)` | Runtime type introspection |
| `panic(msg)` | Halt execution with error |
| Arithmetic on prims | `+`, `-`, `*`, `/` on Int/Float — hardware ops |
| Comparison on prims | `==`, `<`, `>` etc. — hardware ops |
| String operations | `len`, `concat`, indexing — internal representation |
| `List`, `Map`, `Set` constructors | Memory allocation (until self-hostable) |

Everything else should be definable in KS.

## Phasing

### Phase 2a: Language prerequisites

Before any stdlib can exist, KS needs:

1. **Enums / sum types** — `enum Opt[T] { Val(T), Non }`
2. **Generics** — `[T]` type parameters on enums, functions, types
3. **Pattern matching** — `match v { Val(x) => ..., Non => ... }`
4. **Prelude loading** — runtime reads and evaluates `std/prelude.ks` before user code

These are language features, not stdlib. They gate everything below.

### Phase 2b: Prelude — `Opt` and `Res`

Once enums + generics + pattern matching exist:

```ks
enum Opt[T] {
    Val(T),
    None,
}

enum Res[T, E] {
    Val(T),
    Err(E),
}
```

That's it. They're just enums. No special runtime support needed.

Utility methods (`.unwrap()`, `.map()`, `.unwrap_or()`) can be added as the method system matures, but the types themselves are day-one.

### Phase 2c: Core utilities

- `assert(cond, msg)` — could be KS: `func assert(cond, msg) { if !cond { panic(msg) } }`
- `dbg(v)` — `print(type_of(v), ": ", v)` — needs `type_of` intrinsic
- `todo(msg)` — `panic("not yet implemented: " + msg)`

### Phase 3+: Self-hosted collections

When the type system is mature enough, `List`, `Map`, `Set` migrate from runtime intrinsics to KS definitions backed by lower-level primitives (arrays, hash tables). This is the original Phase 4 vision from the roadmap — it just starts earlier.

## Open questions

- **Prelude loading mechanism:** Does the evaluator read `std/prelude.ks` from disk at startup? Embed it at compile time via `include_str!`? The latter is simpler for distribution (single binary) but harder to iterate on.
- **Method syntax on enums:** `opt.unwrap()` needs either a method dispatch system or standalone functions like `unwrap(opt)`. Methods are nicer but require more machinery.
- **Nil ↔ Opt relationship:** See [prop: nil-option](../prop/nil-option.md). If `nil` stays as a prim, it's independent of `Opt.Non`. If it becomes sugar, every type is implicitly optional.
- **Error ↔ Res relationship:** See [prop: error-handling](../prop/error-handling.md). Runtime errors (type mismatches, etc.) might produce `Res.Err` or might remain a separate panic mechanism.
