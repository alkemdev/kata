# TODO: structured error type

## What

Replace `Err(String)` in the interpreter with a structured error enum. The "no panics — return `Err(String)`" invariant kept us safe from panics, but `String` errors are themselves a string hack per the "rich data models" invariant.

## Why now-ish

Error handling is on the Phase 2 "not yet" list. When it lands, the internal error representation should already be structured — otherwise we'll be parsing format strings to distinguish error kinds, which is exactly the pattern the `Spanned<Expr>` refactor taught us to avoid.

## Scope

- Define an error enum (span info, error kind, message)
- Migrate `Err(String)` call sites incrementally
- User-facing error formatting stays in one place
