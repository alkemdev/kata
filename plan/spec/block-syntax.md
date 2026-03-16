# Decision: block expression syntax
**ID:** block-syntax
**Status:** done
**Date opened:** 2026-03-14
**Date done:** 2026-03-14
**Affects:** lexer, parser, eval, syntax

## Question
What syntax should KataScript use for block expressions (scoped groups of statements that produce a value)?

## Context
KataScript needs block expressions for scoping, and `{ k: v }` is desired for map literals. This creates an ambiguity if bare `{}` is used for both blocks and maps — `{ x: 1 }` could be either. The parser cannot resolve this without context-dependent rules (the JavaScript approach, widely regarded as a mistake).

The language is expression-oriented, so blocks should produce a value (the last expression in the block). This enables `let x = <block>` patterns and is needed for `if`/`else` as expressions later.

Additionally, a block keyword could support optional bindings — scoped `let`-like introductions that are only visible within the block body. This is useful for limiting variable lifetimes and is a natural fit for future resource management patterns.

## Alternatives

### Option A: Bare `{}` for blocks, sigil for maps
Blocks use `{}`. Maps get a distinct literal syntax: `#{ k: v }`, `%{ k: v }`, or `[k: v]`.
**Pros:** Familiar block syntax from C/Rust/JS; no new keyword; maps are less common than blocks.
**Cons:** Sigils are ugly; `[k: v]` conflicts with potential slice/index syntax; maps feel second-class.

### Option B: `do { ... }` for blocks, `{ k: v }` for maps
`do` introduces a block. Maps get bare `{}`.
**Pros:** Unambiguous; `do` is well-known (Lua, Ruby, Haskell); short (2 chars); maps get clean syntax.
**Cons:** `do` is 2 chars, breaks the 4-char keyword pattern; every `if`/`while`/`func` body would use `{}` directly but standalone scopes need `do {}` — inconsistent.

### Option C: `with { ... }` for blocks, `{ k: v }` for maps
`with` introduces a scoped block, optionally with bindings:
```ks
// bare scope
with {
    let x = 1
    print(x)
}

// scoped bindings — variables only visible inside
with x = compute(), y = other() {
    print(x, y)
}
// x, y not visible here

// expression — returns last value
let result = with x = parse(input) {
    transform(x)
}
```
**Pros:** 4-char keyword family (`func`/`type`/`kind`/`with`/`enum`); binding form is genuinely useful for scoping and future resource management; reads naturally ("with x equals y, do this"); unambiguous with maps; bare form is just the zero-binding case.
**Cons:** More verbose than bare `{}` for simple blocks; `with` means resource management in Python (`with open(...) as f`), could set wrong expectations; every scope needs a keyword.

### Option D: `do { ... }` for blocks, `with` for bindings
Split the roles: `do { ... }` for bare scope blocks, `with x = ... { }` for scoped bindings. Maps get `{ k: v }`.
**Pros:** Each keyword does one thing; `do` is minimal for bare scoping; `with` only appears when bindings are needed.
**Cons:** Two keywords for closely related concepts; `do` breaks 4-char pattern.

### Option E: `{ ... }` for both, context-dependent parsing
Parse `{` and look ahead: if the pattern is `ident : expr ,`, it's a map; otherwise it's a block.
**Pros:** No new keywords; both blocks and maps get clean syntax.
**Cons:** The JavaScript problem — `{ x: 1 }` is ambiguous (label + expression vs map entry); requires complex lookahead; surprising edge cases; parser complexity leaks into user mental model.

### Option F: `with { ... }` for blocks, `if`/`while`/`func` use bare `{}`
`with` is only needed for standalone scoped blocks. Control flow and function bodies use `{}` directly after their headers (e.g., `if cond { ... }`, `func f() { ... }`). Maps use `{ k: v }` but are only valid in expression position.
**Pros:** Minimal keyword usage — `with` only where there's real ambiguity; control flow stays clean; map literals stay clean.
**Cons:** Two block syntaxes (keyword vs bare) depending on context; must define "expression position" vs "statement position" rules.

## Discussion
**Current state (2026-03-14):** No blocks, no maps, no control flow. `Token::LBrace`/`Token::RBrace` exist in the lexer but are unused by the parser.

The core tension is between blocks and maps for `{}`. Every option either adds a keyword for blocks, adds a sigil for maps, or embraces ambiguity.

**Option E (context-dependent) is out.** JavaScript proved this is a mistake. The parser should know unambiguously what it's looking at from the first token.

**Option A (sigil for maps)** sacrifices map ergonomics for block familiarity. Maps are common enough in a scripting language that they deserve clean syntax. `#{ k: v }` works but feels like a wart.

**Option B (`do`)** is clean but `do` is 2 chars in a language trending toward 4-char declaration keywords. It also creates a split: `if cond { ... }` uses bare braces but standalone scopes need `do { ... }`. Why is one different from the other?

**Option C (`with`)** is the most interesting. The binding form (`with x = expr() { ... }`) is genuinely useful — it's not just a block keyword with sugar, it solves a real scoping problem. And `with` at 4 chars fits the keyword family. The Python association (resource management) isn't necessarily wrong — `with file = open("x") { ... }` with auto-cleanup is a natural future extension.

**Option D** splits `do`/`with` — clean separation but two keywords for one concept feels redundant.

**Option F** is pragmatic: `with` only for standalone scoped blocks, bare `{}` after control flow keywords. This minimizes keyword noise. The rule is simple: `{}` after `if`/`while`/`func`/etc. is always a block body; standalone `{}` in expression position is a map; `with` introduces a standalone scope. But "expression position" vs "statement position" rules can be subtle.

Leaning toward **Option C** — `with` for all standalone blocks (with optional bindings), bare `{}` after control flow headers, `{ k: v }` for maps. The question is whether `if cond { ... }` and `with { ... }` feel consistent enough, or whether the asymmetry is a problem.

Note: the binding form also pairs naturally with pattern matching later:

```ks
with Opt.Some(x) = maybe_value() {
    print(x)
}
```

**Resolution:** No real asymmetry. `with` is its own construct for scoped bindings — the bare no-bindings case is just the edge case. Control flow keywords (`if`/`while`/`func`) already introduce their own context, so bare `{}` after them is unambiguous and natural. Putting `with` everywhere else would be noise.

## Decision

**Chosen:** Option F — `with { ... }` for standalone scoped blocks, bare `{}` after control flow/function headers, `{ k: v }` for maps.
**Rationale:** `with` is a scoped-binding construct, not a general block keyword. Control flow and function bodies don't need it because their keywords already disambiguate. This keeps maps clean (`{ k: v }`), avoids the JavaScript ambiguity, and `with` at 4 chars fits the keyword family. The no-bindings form (`with { ... }`) works for bare scoping but is the edge case, not the primary use.
**Consequences:**

- `with` keyword added to the lexer
- `if`/`while`/`func` bodies use bare `{}` — no `with` required
- `{ k: v }` is reserved for map literals (future)
- Standalone `{ ... }` without a preceding keyword is a parse error (or a map) — never a block
- The binding form (`with x = expr { ... }`) provides scoped variable introduction

## References
- [spec: func-vs-fn](func-vs-fn.md) — 4-char keyword family precedent
- JavaScript block/object ambiguity — https://2ality.com/2012/09/expressions-vs-statements.html
- Rust block expressions — always `{}`, no map literals
- Python `with` statement — resource management with `__enter__`/`__exit__`
- Lua `do ... end` — explicit scope blocks
- Swift `if let` — scoped binding pattern
