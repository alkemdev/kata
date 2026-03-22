# Decision: error handling strategy

**ID:** error-handling
**Status:** decided
**Date opened:** 2026-03-14
**Date decided:** 2026-03-22
**Affects:** eval, syntax, stdlib

## Decision

**Option A: `Res[T, E]` as the primary error mechanism.** No try/catch, no stack unwinding, no exception system.

### Error model

Two failure modes, two mechanisms:

- **Recoverable errors** → `Res[T, E]`. Functions that can fail return `Res[T, E]`. Callers handle errors via `?`, `match`, or methods like `unwrap()`.
- **Programmer errors** → `panic()`. Strictly fatal, uncatchable. Index out of bounds, assertion failures, logic bugs. No boundary mechanism to catch panics.

### `?` operator

The `?` operator works on any 2-variant enum with the right structural shape:

- **Opt-like** (Val + Non): `val?` unwraps `Val(x)` to `x`, early-returns `Non`.
- **Res-like** (Val + Err): `val?` unwraps `Val(x)` to `x`, early-returns `Err(e)`.

Detection is structural via `TryShape` classifier — not hardcoded to Opt/Res by name. Any user-defined 2-variant enum with `Val`+`Non` or `Val`+`Err` works with `?`.

### Error propagation

Propagation is as-is: `?` on `Res.Err(e)` early-returns the whole `Res.Err(e)` value unchanged. No auto-conversion of error types (no Rust `From` equivalent yet). If the returned `Res` type doesn't match the enclosing function's return type annotation, the existing return type check catches the mismatch.

### Res methods

```ks
impl Res[T, E] {
    func unwrap(self): T        # panics on Err
    func unwrap_or(self, default: T): T
    func unwrap_err(self): E    # panics on Val
    func is_val(self): Bool
    func is_err(self): Bool
}
```

### Future extensions

- `From[T]` interface for automatic error type conversion in `?`
- `map`, `map_err`, `and_then` methods (require higher-order function ergonomics)
- Error interface/protocol for structured error types

## Why not try/catch?

- Hidden control flow: any function might throw, callers can silently ignore errors
- Not typed: function signatures don't declare failure modes
- Stack unwinding is complex runtime machinery for a tree-walk interpreter
- Blurs the line between recoverable and unrecoverable errors

## References

- Rust `Result<T, E>` + `?` operator — primary inspiration
- Zig error unions — structural error composition
- Elm `Result err ok` — compiler-enforced error handling
