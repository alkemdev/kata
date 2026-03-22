# Decision: type aliases
**ID:** type-aliases
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** parser, interpreter

## Question
Should KataScript support type aliases like `type Str = Array[Char]` or `type Result = Res[Int, Str]`?

## Context
The `type` keyword is currently used for interface definitions (`type Iter[T] { func next(self): Opt[T] }`). Type aliases are a different concept — giving a shorter name to an existing type.

Without aliases, users must write `Res[Int, Str]` everywhere. With aliases: `type MyResult = Res[Int, Str]` then `func parse(): MyResult { ... }`.

## Alternatives

### Option A: `alias` keyword
`alias Result = Res[Int, Str]` — separate keyword to avoid confusion with `type` (interfaces).
**Pros:** Unambiguous. No parser conflict.
**Cons:** New keyword.

### Option B: Overload `type` keyword
`type Result = Res[Int, Str]` (with `=`) vs `type Iter[T] { ... }` (with `{`). Parser distinguishes by lookahead.
**Pros:** Fewer keywords. Familiar from TypeScript.
**Cons:** Two different meanings for `type`. Could confuse users.

### Option C: `let` for types
Types are first-class values, so `let MyResult = Res[Int, Str]` already works at runtime (it binds the type value to a name). This is essentially a type alias.
**Pros:** Already works! No new syntax needed.
**Cons:** Not declarative — it's a runtime binding, not a compile-time alias. Doesn't compose with type annotations the same way.

## Discussion
Option C is interesting — since types are values, `let` already provides type aliasing. The question is whether we want something more formal. For now, Option C may be sufficient.

## Decision
<!-- blank while open -->

## References
- Rust `type Alias = Original;`
- TypeScript `type Alias = Original`
- KataScript types-as-values: `print(Int)` works
