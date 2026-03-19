# Decision: string interpolation and escape sequences

**ID:** string-interpolation
**Status:** done
**Date opened:** 2026-03-18
**Date done:** 2026-03-18
**Affects:** lexer, parser, eval, syntax

## Question

How should KataScript handle escape sequences and expression interpolation inside string literals?

## Context

Strings were bare double-quoted literals with no escape processing (`"[^"]*"` regex in the lexer). You couldn't embed a quote, a newline, or a computed value in a string. String concatenation via `+` existed but is verbose for building messages:

```ks
// before: concatenation only
print("hello " + name + ", 1+1=" + str(1+1))

// after: interpolation
print("hello {name}, 1+1={1+1}")
```

Escape sequences and interpolation are tightly coupled — both require replacing the regex with a manual scanner, and `\{` is needed to escape literal braces once `{` triggers interpolation.

## Alternatives

### Option A: Prefix-triggered interpolation (`f"..."` or `$"..."`)

Only strings with a prefix allow `{expr}`. Plain strings are left alone.
**Pros:** Backward compatible — existing strings with literal `{` don't change meaning. Explicit opt-in, familiar from Python f-strings and C# `$""`.
**Cons:** Extra ceremony for the common case. In a new language with no existing codebase, backward compatibility is irrelevant. Two string modes is more complexity than one.

### Option B: All strings support `{expr}` interpolation

`{` always starts interpolation in any string. `\{` escapes a literal brace.
**Pros:** Simplest mental model — one string type, one set of rules. No prefix to forget. Every string is "interpolation-ready."
**Cons:** Can't write a literal `{` without escaping. If KataScript later gets format strings or template syntax, this design is already taken.

### Option C: Backtick strings for interpolation

`` `hello {name}` `` for interpolated strings, `"..."` for plain. Backtick strings process `{expr}`, double-quoted strings don't.
**Pros:** Familiar from JavaScript template literals. Plain strings stay simple.
**Cons:** Backtick is visually subtle and easy to confuse with single quotes. Two string syntaxes. Backtick has no clear relationship to "interpolation."

### Option D: `#` or `$` inside the string (`"hello #{name}"`)

Interpolation triggered by `#{expr}` or `${expr}` within any string.
**Pros:** Familiar from Ruby (`#{}`) and shell (`${}`). Literal `{` is unambiguous without escaping.
**Cons:** Two-character trigger is noisier. `#` conflicts with potential comment-in-expression syntax. `$` has no other meaning in KataScript.

### Option E: Double quotes interpolate, single quotes don't

`"hello {name}"` interpolates. `'hello {name}'` is literal. Both process escape sequences.
**Pros:** No prefix, no sigil — the quote character itself signals the mode. Familiar from Ruby, PHP, Perl. Literal `{` needs no escaping in single-quoted strings. Users who don't want interpolation have a natural, easy-to-type alternative.
**Cons:** Two string syntaxes. Users must know the distinction. Risk of accidentally using the wrong quote style.

## Discussion

KataScript has no existing codebase, so backward compatibility isn't a factor. The language is expression-oriented, so restricting interpolation to names (no expressions) would feel arbitrary — `"{1+1}"` should work just like `"{x}"`.

The key tension is between Option A (prefix) and Option B (always-on). Python went with prefixes because plain strings predate f-strings by decades. Rust uses `format!()` macros. Swift uses `\(expr)`. Kotlin, Dart, and most modern scripting languages use always-on `"...$expr..."` or `"...{expr}..."` syntax.

For a new scripting language, always-on is the right default. The prefix adds friction for zero benefit — there is no legacy code to protect.

Option B's cost is that literal `{` requires `\{`. This is rare in user-facing strings but annoying when it comes up — `\{` is hard to type and easy to forget. **Option E resolves this cleanly.** Single-quoted strings give you a natural escape hatch: `'{not interpolated}'` with zero ceremony. The quote character distinction is well-established (Ruby, PHP, Perl) and easy to internalize: double = dynamic, single = literal.

Both quote styles produce the same `Value::Str` at runtime — the distinction is purely lexical.

**Nested strings and braces.** The double-quote scanner must handle `"{Point { x: 1 }}"` (brace depth tracking) and `"outer {"inner"}"` (nested string literals inside interpolation). This is solvable with a simple state machine in the lexer — track brace depth and skip over quoted strings while scanning for the closing `}`.

**Escape sequences.** Both quote styles process: `\n`, `\t`, `\\`, `\"`, `\'`. Double-quoted strings additionally process `\{` and `\}`. Unknown escapes pass through literally (`\q` -> `\q`).

**Display formatting.** Interpolated values use `Value::display(&TypeRegistry)` — the same formatting as `print()`. This means `"{42}"` produces `"42"`, `"{true}"` produces `"true"`, struct values show their fields, etc. No separate formatting protocol needed yet.

## Decision

**Chosen:** Option E — double-quoted strings interpolate, single-quoted strings don't. Both process escape sequences.
**Rationale:** Gives users a natural, zero-ceremony way to write literal strings with braces. The quote-character distinction is well-established across languages and easy to internalize. No prefix needed, no hard-to-type escape sequences for the common "I just want a literal string" case.
**Consequences:**

- Lexer uses two manual scanners (logos callbacks): `lex_double_string` (escapes + interpolation) and `lex_single_string` (escapes only)
- Both produce `Token::Str(Vec<StringPart>)` — single-quoted strings always yield `Lit` parts only
- `Token::Str(String)` becomes `Token::Str(Vec<StringPart>)` where `StringPart` is `Lit(String)` or `Interp(String)`
- Parser recursively lexes and parses interpolation fragments via `parse_fragment`
- `Expr::Interp { parts: Vec<InterpPart> }` added to the AST; plain strings remain `Expr::Str`
- Escape sequences in both: `\n`, `\t`, `\\`, `\"`, `\'`
- Additional escapes in double-quoted only: `\{`, `\}`
- Nested strings and braced expressions inside `{...}` work via depth tracking (both `"..."` and `'...'` are tracked inside interpolation)
- Bad expressions inside `{...}` are parse errors reported via ariadne (with a literal fallback AST node so parsing continues)
- Future: custom `Display`-like protocol for user-defined formatting is deferred

### Examples

```ks
// ── single-quoted strings (literal, no interpolation) ──

print('hello world')           // hello world
print('{not interpolated}')    // {not interpolated}
print('escapes work: \t\n')    // escapes work: <tab><newline>

// ── double-quoted strings (escape sequences + interpolation) ──

print("tab:\there")            // tab:<tab>here
print("line1\nline2")          // line1<newline>line2
print("say \"hi\"")            // say "hi"
print("a\\b")                  // a\b

// variable interpolation
let name = "world"
print("hello {name}")          // hello world

// expression interpolation
print("1+1={1+1}")             // 1+1=2

// multiple interpolations
let a = 1
let b = 2
print("{a} + {b} = {a+b}")    // 1 + 2 = 3

// function call in interpolation
func greet(name: Str): Str { ret "hi {name}" }
print("{greet("world")}")     // hi world

// nested interpolation
let x = "inner"
print("outer {"wrapped {x}"}") // outer wrapped inner

// single-quoted strings inside interpolation
func echo(s: Str): Str { ret s }
print("{echo('arg')}")         // arg

// escaped braces in double-quoted strings
print("literal \{braces\}")    // literal {braces}

// bad expressions are parse errors (not silent fallback)
print("{1 ++}")                // → parse error: invalid expression in string interpolation
```

## References

- Ruby string literals — `"interpolated #{expr}"` vs `'literal'`
- PHP string literals — `"interpolated $var"` vs `'literal'`
- Perl string literals — `"interpolated $var"` vs `'literal'`
- Python f-strings (PEP 498) — prefix-triggered, `f"hello {name}"`
- Kotlin string templates — always-on, `"hello $name"` / `"hello ${expr}"`
- Swift string interpolation — always-on, `"hello \(name)"`
- Rust `format!()` — macro-based, not in-string
- JavaScript template literals — backtick-triggered, `` `hello ${name}` ``
- [spec: block-syntax](block-syntax.md) — `{}` reserved for maps, no ambiguity with string `{}`
