# Decision: pipe operator
**ID:** pipe-operator
**Status:** open
**Date opened:** 2026-03-22
**Date done:** —
**Affects:** lexer, parser

## Question
Should KataScript have a pipe operator (`|>` or similar) for function chaining?

## Context
Method chaining works naturally for struct methods: `arr.push(1).get(0)`. But for free functions, nesting is the only option: `print(double(parse(input)))`. A pipe operator reverses this: `input |> parse |> double |> print`.

## Alternatives

### Option A: `|>` operator (Elixir/F# style)
`x |> f` → `f(x)`. Left-to-right data flow.
**Pros:** Familiar. Clear data flow. Reduces nesting.
**Cons:** New token `|>`. What about multi-arg functions? `x |> f(_, extra)` needs partial application or placeholder syntax.

### Option B: Method-style universal call syntax
`x.f()` where `f` is a free function — first arg becomes the receiver.
**Pros:** No new operator. Uses existing `.` syntax.
**Cons:** Ambiguous — is `x.f()` a method call or a universal call? Breaks encapsulation expectations.

### Option C: Defer
Wait for lambdas and higher-order functions. Pipe is sugar — the underlying capability matters more.
**Pros:** Less to learn. Lambdas + method chaining may be sufficient.
**Cons:** Nested function calls remain verbose.

## Discussion
The pipe operator is most valuable when combined with lambdas and partial application. Without those, pipe is limited to single-arg functions. Consider implementing lambdas first, then evaluating whether pipe is still needed.

## Decision
<!-- blank while open -->

## References
- Elixir `|>`, F# `|>`, OCaml `|>`
- Rust (rejected RFC), JavaScript (TC39 proposal)
