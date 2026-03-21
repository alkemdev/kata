# Decision: error rendering philosophy
**ID:** error-rendering
**Status:** decided
**Date opened:** 2026-03-21
**Date done:** 2026-03-21
**Affects:** interpreter, error reporting

## Principles

1. **The header explains WHAT went wrong.** The `Error:` line is the full diagnostic, readable in isolation.

2. **The primary span points to the CAUSE.** The exact token that caused the error, not the enclosing expression.

3. **Secondary labels annotate CONTEXT.** Type annotations on operands, expected vs actual. Never repeat the header.

4. **Never repeat the header as a label.** If there are secondary labels providing context, the primary span gets no message — the header is enough.

5. **Point to what the user should change.** Wrong type → the wrong value. Missing field → the type name. Undefined → the name.

## Examples

```
Error: cannot apply '+' to Int and Str
   ╭─[test.ks:1:9]
   │
 1 │ let x = 1 + "a"
   │         ┬────┬─
   │         ╰──────── Int
   │              ╰─── Str
───╯

Error: type mismatch: expected Int, got Str
   ╭─[test.ks:1:14]
   │
 1 │ Opt[Int].Val("wrong")
   │              ───┬───
   │                 ╰───── type mismatch: expected Int, got Str
───╯
```

## Decision

Errors with secondary labels (binop, future multi-span errors): header only on the `Error:` line, secondary labels provide type/context annotations, primary span highlights the location without repeating the message.

Errors without secondary labels: the primary span label carries the error message (single annotation).
