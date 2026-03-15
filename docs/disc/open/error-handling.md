# Decision: error handling strategy
**ID:** error-handling
**Status:** open
**Date opened:** 2026-03-14
**Date done:** —
**Affects:** eval, syntax, stdlib

## Question
Should KataScript use `Res[T, E]`, exceptions (`try/catch`), or a hybrid for error handling?

## Context
KataScript's evaluator currently returns `Err(String)` for runtime errors, which immediately halts execution. There's no user-facing error handling mechanism yet — no `try/catch`, no `Result` type, no way for KataScript code to recover from errors. This decision defines how errors surface to the programmer.

The language is expression-oriented, which influences the design: `try` as an expression is natural, and error handling constructs should produce values rather than being purely control-flow.

## Alternatives

### Option A: `Res[T, E]`
Errors are values. Functions that can fail return `Res[T, E]`. Callers must handle the error case explicitly. `Res` is a regular enum defined in KS: `enum Res[T, E] { Ok(T), Err(E) }`.
**Pros:** Explicit; errors are data; composable; no hidden control flow; Rust has validated this approach; no special runtime support needed.
**Cons:** Verbose without pattern matching and `?` operator; `?` is harder to define in a dynamic language (what's the "error type" of the enclosing function?); requires good ergonomics to not feel like Go.

### Option B: `try/catch/throw`
Exceptions with stack unwinding. `throw` raises, `try/catch` handles. Familiar from Python/JS/Java.
**Pros:** Familiar; concise happy path; `try` as expression fits KataScript's expression-oriented design; no return type pollution.
**Cons:** Hidden control flow; any function might throw; hard to know what errors to catch; performance cost of stack unwinding.

### Option C: Go-style multiple returns
Functions return `(value, error)` tuples. Callers check the error.
**Pros:** Explicit; simple; no special syntax needed.
**Cons:** Verbose; easy to ignore the error; no language enforcement; doesn't compose well; universally regarded as Go's worst feature.

### Option D: Hybrid — `Res` for recoverable, exceptions for panics
`Res[T, E]` for expected failures (file not found, parse error). `panic` for programmer errors (index out of bounds, type mismatch). `try` can catch panics but the default is to crash.
**Pros:** Two failure modes mapped to two mechanisms; familiar from Rust; explicit where it matters.
**Cons:** Two error mechanisms to learn; boundary between "recoverable" and "panic" is subjective; more complex implementation.

## Discussion
**Current state (2026-03-14):** The evaluator uses `Err(String)` internally. Runtime errors (type mismatches, undefined variables, division by zero) all go through this path and immediately halt. There's no mechanism for KataScript code to intercept errors.

Expression-oriented design makes `try` as an expression natural:
```
let result = try parse(input) catch err { default_value }
```
This is cleaner than statement-based `try/catch` blocks because it produces a value.

Key constraint: `Res[T, E]` is a regular enum defined in KS (`enum Res[T, E] { Ok(T), Err(E) }`), not a runtime primitive. This means the `?` operator and any `try` sugar need to work with user-defined enums, not hardcoded types.

The `?` operator is elegant in Rust because the compiler knows the return type and can auto-convert errors. In a dynamic language, `?` would need to either:
1. Propagate the `Res.Err` as-is (simpler, but loses context)
2. Require the enclosing function to declare its error type (adds ceremony)
3. Just re-throw as an exception (blurs the Res/exception line)

The `kind` system could define an "Error" form — any type conforming to `Error` can be used as the `E` in `Res[T, E]`. This gives structure without requiring a specific error base class.

Option B (`try/catch`) is the path of least resistance for a dynamic scripting language. Option D (hybrid) is more principled but adds complexity. Option A (`Res` only) needs pattern matching and good ergonomics to not be painful.

Worth noting: if KataScript gets `kind`-based structural typing, the distinction between Options A and D blurs — `Res` and `try/catch` could unify under a "Fallible" kind.

## Decision
<!-- blank while open -->

## References
- [disc: type-system](type-system.md) — overall type architecture
- [disc: nil-option](nil-option.md) — related design question on absence
- Rust `Result<T, E>` + `?` operator
- Python exceptions — `try/except/raise`
- Go multiple returns — `val, err := f()`
- Swift `throws` + `try` + `Result` hybrid
