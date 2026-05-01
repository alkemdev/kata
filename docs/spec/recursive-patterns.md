# Decision: recursive patterns and destructuring
**ID:** recursive-patterns
**Status:** decided
**Date opened:** 2026-04-09
**Date done:** 2026-04-30
**Affects:** ast, parser, interpreter
**Supersedes:** tuple-type (partially), destructuring (partially), pattern-matching-v2 (partially)

## Question
How should patterns work across `match`, `let`, and `for` so that values can be destructured correctly, with full type checking, no string-based dispatch, and no syntactic ambiguity?

## Decision

**Three-layer architecture:** AST `Pattern` → `ResolvedPattern` (resolution phase) → matching. Variant matching uses `(TypeId, variant_idx: u32)` handles, never strings.

**Unambiguous grammar:** variant patterns always have parens (`Non()`, not `Non`). Bare identifiers are always bindings. No case-based heuristics. Breaking syntax change for unit variants in match arms (migration: `Non` → `Non()`, `Empty` → `Empty()`, etc.). Construction syntax (`Opt[Int].Non`) is unchanged.

**New AST primitives:**
- `Literal` enum (Int/Float/Str/Bool/Nil) replaces `Spanned<Expr>` in `Pattern::Literal`
- `Pattern::Variant { name: Spanned<String>, bindings: Vec<Spanned<Pattern>> }` — recursive
- `Pattern::Tuple(Vec<Spanned<Pattern>>)` — new
- `Stmt::Let { pattern: Spanned<Pattern>, ... }` and `Expr::For { pattern: Spanned<Pattern>, ... }`

**Resolution layer** validates everything against the subject's TypeId before any matching: variant name existence, arity, type/pattern compatibility, irrefutability for let. All errors are structured `ErrorKind` variants with spans.

**Let** allows only irrefutable patterns (Binding, Wildcard, Tuple). Refutable patterns (Variant, Literal) require `match` — `let-else` deferred.

**For** resolves the pattern against the iterator's element TypeId at loop entry, catching errors even on empty iterators.

**Safety net:** if a bare identifier in a match arm matches a variant name of the subject's enum, resolution emits `PatternAmbiguousBinding` pointing at the missing parens — catches the syntax migration silently.

**Out of scope (deferred):** struct patterns, guards, or-patterns, exhaustiveness checking, `let-else`.

---

## Principles

1. **No syntactic ambiguity.** The grammar alone determines whether a pattern is a variant match, a binding, a wildcard, a literal, or a tuple. No runtime guessing, no case conventions, no heuristics.
2. **Structured AST.** Names in patterns are `Spanned<String>` — the string plus its source span. Composes with the existing `Spanned<T>` infrastructure. The AST represents syntax faithfully and supports precise error reporting.
3. **Handle-based matching.** A resolution phase turns names into `TypeId` + `variant_idx` handles. The matching phase uses integer comparison — no string comparisons, no registry lookups.
4. **Eager validation.** Resolution checks all arms before any matching begins. Wrong names, wrong arities, and incompatible pattern shapes are caught immediately with span-accurate errors.

---

## Disambiguation rule

**Variant patterns always have parens. Always.**

```ks
Val(x)     # data variant — parens with sub-patterns
Non()      # unit variant — empty parens
x          # binding — bare ident, always
_          # wildcard
42         # literal
(a, b)     # tuple
```

No ambiguity exists:
- `IDENT(...)` → variant pattern. The parens are mandatory, even when empty.
- `IDENT` (bare) → binding. Always. Regardless of case.
- `_` → wildcard.

This is a breaking syntax change: `Non` in a match arm currently works as a unit variant by runtime heuristic. Under this proposal, `Non` would be a catch-all binding. The migration is mechanical: `Non` → `Non()`, `Empty` → `Empty()`, `Del` → `Del()`.

**Safety net:** during resolution, if a `Binding` pattern's name matches a variant name of the subject's enum type, resolution emits an error:

> `'Non' matches variant name of Opt[Int]; use Non() for variant match`

This catches the migration gap — anyone who writes the old syntax gets a clear error pointing them to the fix, rather than silent wrong behavior.

---

## Pattern grammar

```
pattern  = IDENT '(' pat_list? ')'                        -- variant (always has parens)
         | '(' ')'                                         -- unit tuple
         | '(' pattern ',' ')'                             -- 1-tuple
         | '(' pattern (',' pattern)+ ','? ')'             -- n-tuple
         | '_'                                             -- wildcard
         | literal                                         -- 42, -3, "hi", true, false, nil
         | IDENT                                           -- binding

literal  = '-'? NUM                                        -- signed integer
         | '-'? FLOAT                                      -- signed float
         | STR                                             -- plain string, no interp
         | 'true' | 'false' | 'nil'

pat_list = pattern (',' pattern)* ','?
```

**Tuple disambiguation** same as expressions: `(p)` is grouping, `(p,)` is 1-tuple, `(p, q)` is 2-tuple.

**Negative literals** are part of the literal production — not a unary-op expression. In patterns, `-42` is `Literal::Int("-42")`. The sign is absorbed into the `Literal` value; the parser does not produce a `UnaryOp` node for pattern-position literals. This keeps the AST uniform: a literal pattern holds a `Literal`, period.

**Strings in patterns** must be plain — no interpolation. `match s { "hello" -> ... }` is allowed; `match s { "hi, {name}" -> ... }` is rejected at the lexer-to-parser handoff.

---

## Data structures

This is the authoritative reference for every type involved in patterns. Later sections (Resolution, Matching) describe the algorithms that operate on these types.

### Foundation (existing types)

These already exist; patterns compose with them.

```rust
// Source span — byte offsets into the source file.
pub type Span = (usize, usize);

// A node of type T annotated with its source span.
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

// Opaque handle into TypeRegistry. Cheap to copy.
#[derive(Copy, Clone)]
pub struct TypeId(pub u32);

// Runtime value — relevant variants for pattern matching:
pub enum Value {
    Enum {
        type_id: TypeId,
        variant_idx: u32,       // ← the handle patterns resolve against
        fields: Arc<[Value]>,
    },
    Tup {
        type_id: TypeId,
        fields: Arc<[Value]>,
    },
    Int(Arc<BigInt>),
    Float(f64),
    Str(Arc<str>),
    Bool(bool),
    Nil,
    // ... other variants not relevant to matching
}
```

Key observation: `Value::Enum` already stores `variant_idx: u32`. Matching is comparing this `u32` to a resolved `u32` — no strings in the hot path.

### New: `Literal`

Pattern literals are narrowly restricted — only primitive constants can appear as a literal pattern. Today this is stored as `Spanned<Expr>`, which allows *any* expression in the type system even though the parser only emits literals. A dedicated enum makes the AST say what it means.

```rust
/// The set of values that can appear as a literal pattern.
/// Narrower than Expr — only constants, no interpolation, no computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Literal {
    /// `42`, `-17`, `0xff` — stored as the raw lexed string for BigInt precision.
    Int(String),
    /// `3.14`, `1e-9` — stored as raw string, parsed at resolution time.
    Float(String),
    /// `"hello"` — plain string only, no interpolation allowed in patterns.
    Str(String),
    /// `true`, `false`.
    Bool(bool),
    /// `nil`.
    Nil,
}
```

**Field notes:**
- `Int` and `Float` hold strings (not parsed numbers) to match the existing `Expr::Int(String)` / `Expr::Float(String)` convention. KataScript Ints are arbitrary-precision (BigInt); lexing-time parsing would lose precision for very large literals.
- `Str` holds an owned `String` — these are short constants used once, no need to intern.
- No `Bin` (byte string) variant: byte string literals are allowed in the future but not in this proposal.

**Resolution:** `Literal` is pre-evaluated to a `Value` at resolution time. The raw strings become `Value::Int(Arc<BigInt>)`, `Value::Float(f64)`, etc.

### Modified: `Pattern`

The AST representation of a pattern. This is what the parser produces.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,

    /// `x`, `other` — bare identifier, always a catch-all binding.
    /// The Spanned<String> carries the name text and its span.
    Binding(Spanned<String>),

    /// `42`, `"hello"`, `true`, `false`, `nil`.
    /// Spanned<Literal> carries the literal value and its span.
    Literal(Spanned<Literal>),

    /// `Val(p1, p2)` or `Non()` — variant match, always with parens.
    Variant {
        /// The variant name as the programmer wrote it (e.g. "Val", "Non").
        name: Spanned<String>,
        /// Sub-patterns for each field. Empty vec means unit variant.
        /// Recursive — each binding is a full Pattern.
        bindings: Vec<Spanned<Pattern>>,
    },

    /// `(p1, p2, ...)` — tuple destructure.
    /// Element count = tuple arity. Recursive.
    Tuple(Vec<Spanned<Pattern>>),
}
```

**Per-variant breakdown:**

| Variant | Represents | Fields carry |
|---------|-----------|--------------|
| `Wildcard` | `_` | nothing — span is on the outer `Spanned<Pattern>` |
| `Binding(Spanned<String>)` | `x` | the name text + span of the name |
| `Literal(Spanned<Literal>)` | `42`, `"hi"`, etc. | the literal + span of the literal |
| `Variant { name, bindings }` | `Val(x)`, `Non()` | name with its own span (just the ident) + span-wrapped sub-patterns |
| `Tuple(Vec<Spanned<Pattern>>)` | `(x, y)` | span-wrapped sub-patterns, no separate outer span needed (the outer `Spanned<Pattern>` covers the whole `(...)`) |

**Why `Spanned<String>` on `Binding` when the outer `Spanned<Pattern>` already has a span?**
For `Binding`, the two spans are identical. We keep `Spanned<String>` for structural consistency with `Variant::name` — every name in the pattern AST is `Spanned<String>`. Also it means we can pass the `Spanned<String>` to error reporting uniformly without having to decide "is this a Binding or a Variant name."

**Why `Vec<Spanned<Pattern>>` for recursion?**
Each sub-pattern is a full pattern with its own span. `Spanned` is the standard wrapper across the AST; the pattern tree matches the nesting of the source.

**Memory:** `Pattern` is produced during parsing and cloned only when moved through the parser pipeline. No Arc/Rc needed. Size: a few bytes per variant tag plus the payload. Deep nesting is bounded by source-code depth.

### Modified: `MatchArm` (container for patterns)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Spanned<Pattern>,
    pub body: Vec<Spanned<Stmt>>,
}
```

Unchanged structurally; only the `Pattern` inside changes (richer variants). Kept here for completeness.

### Modified: `Stmt::Let`

```rust
// Before
Stmt::Let { name: String, value: Spanned<Expr> }

// After
Stmt::Let { pattern: Spanned<Pattern>, value: Spanned<Expr> }
```

The plain `name: String` becomes a full `Spanned<Pattern>`. A simple `let x = 42` still works — it parses as `Let { pattern: Spanned(Binding("x")), value: ... }`. A destructure `let (a, b) = pair` parses as `Let { pattern: Spanned(Tuple([a, b])), value: ... }`.

### Modified: `Expr::For`

```rust
// Before
Expr::For {
    binding: String,
    iter_expr: Box<Spanned<Expr>>,
    body: Vec<Spanned<Stmt>>,
}

// After
Expr::For {
    pattern: Spanned<Pattern>,
    iter_expr: Box<Spanned<Expr>>,
    body: Vec<Spanned<Stmt>>,
}
```

Same transformation as `Let`. `for x in xs` becomes `For { pattern: Binding("x"), ... }`. `for (k, v) in map` becomes `For { pattern: Tuple([k, v]), ... }`.

### New: `ResolvedPattern`

The output of resolution. Produced by walking a `Pattern` alongside the subject's type; every variant name has been replaced by its `(TypeId, variant_idx)` handle.

```rust
#[derive(Debug, Clone)]
pub enum ResolvedPattern {
    /// `_` — matches anything, binds nothing.
    Wildcard,

    /// Catch-all binding. The Spanned<String> is preserved so matching can
    /// report the binding's source location for diagnostics (e.g., unused
    /// binding warnings later). Only the String contents become a local
    /// variable; the span is metadata.
    Binding(Spanned<String>),

    /// Literal equality — pre-evaluated to a concrete Value.
    /// Matching compares Values directly; no re-parsing.
    Literal(Value),

    /// Variant match — identified entirely by type + index.
    /// NO variant name here. The name was consumed at resolution time,
    /// replaced by variant_idx, which matches Value::Enum::variant_idx directly.
    Variant {
        /// The enum type this pattern matches. Used for validation and errors.
        type_id: TypeId,
        /// The variant index within the enum — compared against
        /// Value::Enum::variant_idx during matching. u32 equality only.
        variant_idx: u32,
        /// Resolved sub-patterns. Length equals variant arity (verified).
        bindings: Vec<ResolvedPattern>,
    },

    /// Tuple match — arity already verified during resolution.
    /// Sub-patterns correspond positionally to tuple fields.
    Tuple(Vec<ResolvedPattern>),
}
```

**Per-variant breakdown:**

| Variant | AST source | Key change from AST |
|---------|-----------|---------------------|
| `Wildcard` | `Pattern::Wildcard` | Identical |
| `Binding(Spanned<String>)` | `Pattern::Binding(...)` | Identical payload; AST-level validation complete |
| `Literal(Value)` | `Pattern::Literal(Spanned<Literal>)` | Literal parsed into concrete Value; span dropped (errors already reported at resolve time) |
| `Variant { type_id, variant_idx, bindings }` | `Pattern::Variant { name, bindings }` | **Name is gone.** Replaced by `type_id + variant_idx` handles. Sub-patterns recursively resolved. |
| `Tuple(Vec<ResolvedPattern>)` | `Pattern::Tuple(...)` | Span wrappers dropped; arity verified |

**Why drop spans in sub-positions?**
Spans serve error reporting. All errors in resolution are caught in the resolve phase (before matching). During matching, there's no error path that needs span info — either a pattern matches or it doesn't. Dropping spans keeps the resolved tree compact and focuses it on its one job: structural comparison.

Spans are *retained* on `Binding` because the binding name is a programmer-facing location (future: unused-binding warnings, debugger symbol locations).

**Why `Vec<ResolvedPattern>` rather than `Arc<[ResolvedPattern]>`?**
ResolvedPattern lives for one match execution, is not shared across threads, and doesn't need reference counting. `Vec` is the simplest correct choice. If profiling later shows pattern allocation is hot, `Box<[ResolvedPattern]>` is a drop-in replacement.

**Lifetime:**
ResolvedPattern is built per-match-execution (see Resolution section) and discarded when the match completes. It is *not* cached on the AST — the subject's type is known only at match time, and caching would require invalidation machinery we don't need.

### Type relationships

```
Parser output:                   After resolution:
                                 
Pattern                          ResolvedPattern
 ├─ Wildcard                      ├─ Wildcard
 ├─ Binding(Spanned<String>) ──→  ├─ Binding(Spanned<String>)     [preserved]
 ├─ Literal(Spanned<Literal>) ─→  ├─ Literal(Value)                [pre-evaluated]
 ├─ Variant { name, bindings } →  ├─ Variant {                     [name → handle]
 │   ├─ name: Spanned<String>     │   type_id: TypeId,
 │   └─ bindings: Vec<...>        │   variant_idx: u32,
 │                                │   bindings: Vec<ResolvedPattern>
 │                                │ }
 └─ Tuple(Vec<...>) ───────────→  └─ Tuple(Vec<ResolvedPattern>)   [arity checked]

Runtime check (match phase):
  ResolvedPattern + Value  →  Option<Vec<(String, Value)>>
                                       ^
                                       binding name → bound value
```

### Where the strings live (the accounting)

A full accounting of every `String` at each stage:

**In `Pattern` (AST):**
- `Binding`: binding name → becomes a local variable after match
- `Literal::Int/Float`: raw numeric text → parsed to Value at resolution
- `Literal::Str`: the string constant itself → becomes a Value
- `Variant::name`: the variant name as written → resolved to `variant_idx` and dropped

**In `ResolvedPattern`:**
- `Binding`: binding name → becomes a local variable at match time
- `Value::Int`/`Value::Str`/etc. internal strings: part of the Value representation, unchanged

**In `ResolvedPattern::Variant`:**
- **Zero strings.** Only `TypeId` (u32) and `variant_idx` (u32).

This is the structural guarantee: variant matching in the hot path touches no strings.

---

## Layer 2: Resolution

Resolution runs once at match entry (all arms), once per let binding, once at for-loop entry. It walks the AST `Pattern` alongside the subject's type, validates everything, and produces a `ResolvedPattern` where variant names have been replaced by `(TypeId, variant_idx)` handles. See the Data Structures section for the full `ResolvedPattern` definition.

### Resolution function

A single entry point takes a pattern and the expected `TypeId`:

```rust
fn resolve_pattern(
    &self,
    pat: &Spanned<Pattern>,
    expected: TypeId,
) -> Result<ResolvedPattern, RuntimeError>
```

Top-level callers extract the TypeId from the subject via `Value::type_id()`:
```rust
let resolved = self.resolve_pattern(&arm.pattern, subject.type_id())?;
```

Sub-patterns recurse using the relevant element TypeId:
- For `Variant` sub-patterns: `variant_def.fields[i]` (field types from `ResolvedVariantDef`)
- For `Tuple` sub-patterns: `tuple_instance.type_args[i]` (element types from `TupleInstance`)

### Pattern/subject compatibility

Every pattern has a set of subject types it can match. Incompatible combinations are rejected at resolution. `Wildcard` and `Binding` accept any subject.

| Pattern | Compatible subject TypeId |
|---------|---------------------------|
| `Wildcard` | any |
| `Binding` | any |
| `Literal(Int(...))` | `Int` |
| `Literal(Float(...))` | `Float` |
| `Literal(Str(...))` | `Str` |
| `Literal(Bool(...))` | `Bool` |
| `Literal(Nil)` | `Nil` |
| `Variant { .. }` | `EnumInstance` |
| `Tuple(...)` | `TupleInstance` |

For subjects that are records (`StructInstance`), functions, modules, or other opaque values, only `Wildcard` and `Binding` are valid — struct destructuring patterns are deferred.

### Resolution rules

**`Pattern::Variant { name, bindings }`:**
1. `expected` must resolve to an `EnumInstance`. Otherwise → `ErrorKind::PatternTypeMismatch` at `name.span`.
2. Look up `name.node` in the enum's variant table via `types.get_variant(expected, &name.node)` → returns `(variant_idx: u32, &ResolvedVariantDef)`. Not found → `ErrorKind::PatternUnknownVariant` at `name.span`.
3. Check `bindings.len() == variant_def.fields.len()`. Mismatch → `ErrorKind::PatternVariantArity` at the outer pattern span.
4. Recursively resolve each sub-pattern against the corresponding field TypeId: `resolve_pattern(&bindings[i], variant_def.fields[i])`.
5. Produce `ResolvedPattern::Variant { type_id: expected, variant_idx, bindings: resolved_subs }`. The `Spanned<String>` name is consumed — only the resolved handle survives.

**`Pattern::Tuple(pats)`:**
1. `expected` must resolve to a `TupleInstance`. Otherwise → `ErrorKind::PatternTypeMismatch`.
2. Let `type_args` = the tuple's element TypeIds. Check `pats.len() == type_args.len()`. Mismatch → `ErrorKind::PatternTupleArity`.
3. Recursively resolve each sub-pattern: `resolve_pattern(&pats[i], type_args[i])`.
4. Produce `ResolvedPattern::Tuple(resolved_subs)`.

**`Pattern::Binding(name)`:**
1. **Safety check (match context only):** if `expected` resolves to an `EnumInstance`, check if `name.node` matches any variant name in the enum's variant table. If so → `ErrorKind::PatternAmbiguousBinding` at `name.span` (catches the `Non` → `Non()` migration). Skipped in let/for context.
2. Produce `ResolvedPattern::Binding(name.clone())`.

**`Pattern::Literal(lit)`:**
1. Validate type compatibility per the compatibility table above. Mismatch (e.g., `42` against `Str`) → `ErrorKind::PatternTypeMismatch` at `lit.span`.
2. Evaluate `lit.node` (a `Literal`) to a concrete `Value`:
   - `Literal::Int(s)` → parse as BigInt → `Value::Int(Arc::new(bigint))`
   - `Literal::Float(s)` → parse as f64 → `Value::Float(f)`
   - `Literal::Str(s)` → `Value::Str(Arc::from(s))`
   - `Literal::Bool(b)` → `Value::Bool(b)`
   - `Literal::Nil` → `Value::Nil`
3. Produce `ResolvedPattern::Literal(value)`.

**`Pattern::Wildcard`:**
1. Produce `ResolvedPattern::Wildcard`.

### Repeated binding check

After resolving a top-level pattern, walk the resolved tree and collect every `Binding` name. If any name appears twice → `ErrorKind::PatternRepeatedBinding { name, first_span, repeat_span }`. This catches `match pair { (x, x) -> ... }` and `let (a, a) = ...`.

Why post-resolution rather than during: the walk is simple (linear in tree size) and separates concerns — resolution handles type/name correctness against the subject; the repeat check is a pure syntactic/structural constraint on the pattern tree.

Wildcards never participate — `(_, _, _)` is always valid because `_` binds nothing.

### Match: resolve ALL arms, then match

```rust
fn exec_match(&mut self, subject: Value, arms: &[MatchArm]) -> Result<Flow> {
    let subject_ty = subject.type_id();

    // Phase 1: resolve every arm's pattern against the subject's type.
    // Errors in ANY arm are caught here, even if an earlier arm would match.
    let resolved: Vec<(ResolvedPattern, &[Spanned<Stmt>])> = arms.iter()
        .map(|arm| {
            let rp = self.resolve_pattern(&arm.pattern, subject_ty)?;
            Ok((rp, arm.body.as_slice()))
        })
        .collect::<Result<_, RuntimeError>>()?;

    // Phase 2: find first matching arm.
    for (rp, body) in &resolved {
        if let Some(bindings) = self.match_resolved(&subject, rp) {
            // bind variables, execute body
            ...
        }
    }

    Err(ErrorKind::NoMatchArm { subject_ty }.into())
}
```

---

## Layer 3: Matching

Pure structural recursion on `ResolvedPattern`. **No string comparisons. No registry lookups.**

```rust
fn match_resolved(
    &self,
    val: &Value,
    pat: &ResolvedPattern,
) -> Option<Vec<(String, Value)>> {
    match pat {
        ResolvedPattern::Wildcard => Some(vec![]),

        ResolvedPattern::Binding(name) => {
            Some(vec![(name.node.clone(), val.clone())])
        }

        ResolvedPattern::Literal(expected) => {
            if val == expected {
                Some(vec![])
            } else {
                None
            }
        }

        ResolvedPattern::Variant { variant_idx, bindings, .. } => {
            let Value::Enum {
                variant_idx: val_idx,
                fields,
                ..
            } = val else {
                return None;
            };
            // u32 == u32. No strings.
            if val_idx != variant_idx {
                return None;
            }
            let mut result = vec![];
            for (sub_pat, field) in bindings.iter().zip(fields.iter()) {
                result.extend(self.match_resolved(field, sub_pat)?);
            }
            Some(result)
        }

        ResolvedPattern::Tuple(pats) => {
            let Value::Tup { fields, .. } = val else {
                return None;
            };
            // Arity validated at resolution time.
            let mut result = vec![];
            for (sub_pat, field) in pats.iter().zip(fields.iter()) {
                result.extend(self.match_resolved(field, sub_pat)?);
            }
            Some(result)
        }
    }
}
```

---

## Let destructuring

Only irrefutable patterns allowed:

| Pattern | Allowed | Rationale |
|---------|---------|-----------|
| `Binding` | Yes | Always matches. |
| `Wildcard` | Yes | Always matches. |
| `Tuple(...)` | Yes | Irrefutable if all sub-patterns are irrefutable. |
| `Variant { .. }` | **No** | Refutable — deferred to let-else. |
| `Literal(...)` | **No** | Refutable. |

Refutable patterns are rejected **before** standard resolution by `resolve_pattern_irrefutable`, which is structurally identical to `resolve_pattern` but errors on refutable variants:

```rust
fn resolve_pattern_irrefutable(
    &self,
    pat: &Spanned<Pattern>,
    expected: TypeId,
) -> Result<ResolvedPattern, RuntimeError> {
    match &pat.node {
        Pattern::Variant { name, .. } => Err(ErrorKind::PatternRefutableInLet {
            kind: PatternKind::Variant,
            span: name.span,
        }.into()),

        Pattern::Literal(lit) => Err(ErrorKind::PatternRefutableInLet {
            kind: PatternKind::Literal,
            span: lit.span,
        }.into()),

        // Tuple/Binding/Wildcard are irrefutable; recurse with the same
        // type-checking rules as resolve_pattern.
        Pattern::Tuple(pats) => {
            let type_args = self.types.tuple_type_args(expected)
                .ok_or(ErrorKind::PatternTypeMismatch { ... })?;
            if pats.len() != type_args.len() {
                return Err(ErrorKind::PatternTupleArity { ... }.into());
            }
            let resolved = pats.iter()
                .zip(type_args.iter())
                .map(|(p, ty)| self.resolve_pattern_irrefutable(p, *ty))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResolvedPattern::Tuple(resolved))
        }

        Pattern::Binding(name) => Ok(ResolvedPattern::Binding(name.clone())),
        Pattern::Wildcard => Ok(ResolvedPattern::Wildcard),
    }
}
```

The recursion ensures irrefutability at every depth: `let (a, Val(x)) = ...` errors because the inner `Val(x)` is refutable.

### Examples

```ks
let (idx, found) = self._find(key)
let (_, count) = tally(items)
let (a, (b, c)) = (1, (2, 3))
let x = 42                              # Binding(Spanned("x")) — unchanged
```

---

## For-loop destructuring

Same rules as let (only irrefutable patterns). Resolution happens at loop entry against the iterator's element TypeId, obtained from the `Iter[T]` / `ToIter[T]` protocol:

```rust
// Conceptual:
let iter_ty = self.iter_element_type(iter_expr_result)?;  // T from Iter[T]
let resolved = self.resolve_pattern_irrefutable(&for_expr.pattern, iter_ty)?;

for value in iterator {
    let bindings = self.match_resolved(&value, &resolved)
        .expect("irrefutable pattern cannot fail");
    // bind, execute body
}
```

**Empty iterators:** pattern resolution still runs at loop entry, so structural errors (e.g., `for (a, b) in xs` where `xs` yields scalars) are caught even if the loop body never executes.

**Consistency:** because we resolve once against the iterator's element TypeId, every yielded value is expected to share that type. Values yielded that don't match (a dynamically-typed iterator producing heterogeneous values) produce a runtime error via the existing `match_resolved` return-None path, upgraded to a structured error in for-loop context.

```ks
for (key, val) in map {
    print("{key}: {val}")
}
```

---

## Parser changes

### Recursive pattern parser

```rust
let pattern = recursive(|pat| {
    // Variant: IDENT '(' pat_list? ')'  — always has parens
    let variant = select! { Token::Ident(name) => name }
        .map_with(|name, ex| Spanned::new(name, span(&ex.span())))
        .then(
            pat.clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .map_with(|(name, bindings), ex| {
            Spanned::new(Pattern::Variant { name, bindings }, span(&ex.span()))
        });

    // Tuple: same disambig as tup_or_group
    let tuple_pat = just(Token::LParen)
        .then(pat.clone().separated_by(just(Token::Comma)).collect::<Vec<_>>())
        .then(just(Token::Comma).or_not())
        .then_ignore(just(Token::RParen))
        .map_with(|((_, elements), trailing_comma), ex| {
            let s = span(&ex.span());
            if elements.len() == 1 && trailing_comma.is_none() {
                elements.into_iter().next().unwrap()  // (p) — grouping
            } else {
                Spanned::new(Pattern::Tuple(elements), s)
            }
        });

    // Wildcard: _
    let wildcard = select! { Token::Ident(name) => name }
        .filter(|n| n == "_")
        .map_with(|_, ex| Spanned::new(Pattern::Wildcard, span(&ex.span())));

    // Literals: int, string, bool, nil (same as current)
    let literals = /* ... same as current ... */;

    // Binding: bare ident (not _)
    let binding = select! { Token::Ident(name) => name }
        .filter(|n| n != "_")
        .map_with(|name, ex| {
            let s = span(&ex.span());
            Spanned::new(Pattern::Binding(Spanned::new(name, s)), s)
        });

    // Priority: variant > tuple > wildcard > literal > binding
    // variant must precede binding because both start with IDENT,
    // but variant requires following '(' which is unambiguous.
    variant
        .or(tuple_pat)
        .or(wildcard)
        .or(literals)
        .or(binding)
});
```

### BNF update (top of parser.rs)

```
//   let_stmt   = 'let' pattern '=' expr ';'?
//   for_expr   = 'for' pattern 'in' expr '{' stmt* '}'
//   pattern    = IDENT '(' pat_list? ')'                     -- variant (always parens)
//              | tup_pat                                      -- tuple
//              | '_'                                          -- wildcard
//              | pat_literal                                  -- literal
//              | IDENT                                        -- binding
//   pat_literal= '-'? NUM | '-'? FLOAT | STR | 'true' | 'false' | 'nil'
//   pat_list   = pattern (',' pattern)* ','?
//   tup_pat    = '(' ')' | '(' pattern ',' ')' | '(' pattern (',' pattern)+ ','? ')'
```

---

## Migration

### Syntax

| Old syntax | New syntax | AST change |
|-----------|-----------|------------|
| `Val(x)` | `Val(x)` | bindings: `Vec<String>` → `Vec<Spanned<Pattern>>` |
| `Non` (bare) | `Non()` | Was `Binding("Non")` → now `Variant { name: Spanned("Non"), bindings: [] }` |
| `Empty` (bare) | `Empty()` | Same |
| `Del` (bare) | `Del()` | Same |
| `_` | `_` | Unchanged |
| `other` | `other` | `Binding(String)` → `Binding(Spanned<String>)` |
| `let x = ...` | `let x = ...` | `Let { name }` → `Let { pattern: Binding(Spanned("x")) }` |
| `for x in ...` | `for x in ...` | `For { binding }` → `For { pattern: Binding(x) }` |

### Files requiring unit variant syntax update

Audited against all `.ks` files in `tests/`, `std/`, and `demos/`:

```
std/core/opt.ks                  — Non → Non()                           (2 match arms)
std/dsa/map.ks                   — Empty → Empty(), Del → Del()          (3 match arms)
demos/zoo/zoo.ks                 — Non → Non(), Fish → Fish()            (3 match arms)
tests/ks/match/basic_variant.ks  — Non → Non()                           (2 arms)
tests/ks/match/multi_field.ks    — Empty → Empty()                       (1 arm)
tests/ks/match/expression.ks     — Red/Green/Blue → Red()/Green()/Blue() (3 arms)
tests/ks/enum/data_variant.ks    — Non → Non()                           (if present in match)
tests/ks/enum/generic_none.ks    — Non → Non()                           (if present in match)
tests/ks/enum/wrong_arity.ks     — Non → Non()                           (if present in match)
```

Note: enum **definitions** are unchanged (still `Non`, `Empty`, etc.). Only pattern **matches** in match arms get parens. Construction syntax `Opt[Int].Non` is also unchanged — see "Construction vs pattern syntax" below.

The safety-net error (`PatternAmbiguousBinding`: `"'Non' is a variant of Opt[Int]; use Non() for variant match"`) catches any missed migration sites at runtime.

### Construction vs pattern syntax

These are two distinct syntaxes that look similar but serve opposite purposes:

| Use | Unit variant | Data variant |
|-----|-------------|--------------|
| **Construction** (creating a value) | `Opt[Int].Non` | `Opt[Int].Val(42)` |
| **Pattern** (matching a value) | `Non()` | `Val(x)` |

Construction reads a variant from a type (dot-access on a type value), then optionally calls it with arguments. No parens are needed for unit variants because there are no arguments to pass. Pattern syntax, in contrast, uses parens to disambiguate from a bare identifier (binding). This proposal changes only pattern syntax; construction is untouched.

---

## Error types

All pattern errors are structured `ErrorKind` variants carrying typed data (TypeIds, names, spans). Formatting into human-readable messages is deferred to render time — consistent with the existing `ErrorKind` pattern. New variants:

```rust
pub enum ErrorKind {
    // ... existing variants

    /// Variant pattern references a name that isn't a variant of the subject's enum.
    /// Example: `Vla(x)` on `Opt[Int]`.
    PatternUnknownVariant {
        type_id: TypeId,
        variant_name: String,
        span: Span,
    },

    /// Variant pattern's binding count doesn't match the variant's field count.
    /// Example: `Val(x, y)` on a 1-field variant.
    PatternVariantArity {
        type_id: TypeId,
        variant_name: String,
        expected: usize,
        actual: usize,
        span: Span,
    },

    /// Tuple pattern length doesn't match tuple arity.
    PatternTupleArity {
        expected: usize,
        actual: usize,
        span: Span,
    },

    /// Pattern shape is incompatible with subject type.
    /// Example: tuple pattern on Int, variant pattern on Rec, literal on enum.
    PatternTypeMismatch {
        pattern_kind: PatternKind,
        subject_type: TypeId,
        span: Span,
    },

    /// Bare identifier in match matches a variant name of the subject's enum —
    /// likely forgot parens on a unit variant.
    PatternAmbiguousBinding {
        binding_name: String,
        type_id: TypeId,
        span: Span,
    },

    /// Refutable pattern appears in `let` or `for` (which require irrefutable).
    PatternRefutableInLet {
        kind: PatternKind,
        span: Span,
    },

    /// Same binding name appears more than once in a single pattern.
    /// Example: `match p { (x, x) -> ... }` or `let (a, a) = ...`.
    PatternRepeatedBinding {
        name: String,
        first_span: Span,
        repeat_span: Span,
    },

    /// Match expression exhausted all arms without finding a match.
    NoMatchArm {
        subject_ty: TypeId,
    },
}

/// Categorizes patterns for error reporting.
#[derive(Debug, Clone, Copy)]
pub enum PatternKind {
    Wildcard,
    Binding,
    Literal,
    Variant,
    Tuple,
}
```

Each variant carries the minimum raw data needed to produce a high-quality error message at render time. No string formatting happens in the interpreter — only structured data capture.

---

## Error messages

Rendered forms of the structured errors above. All errors carry the span of the offending node — `Spanned<String>.span` for names, `Spanned<Pattern>.span` for compound patterns.

### Resolution errors

| Situation | ErrorKind | Rendered |
|-----------|-----------|----------|
| `Vla(x)` on `Opt[Int]` | `PatternUnknownVariant` | `no variant 'Vla' on type Opt[Int]` |
| `Val(x, y)` on 1-field variant | `PatternVariantArity` | `variant 'Val' of Opt[Int] has 1 field but pattern has 2` |
| `Val(x)` on non-enum (e.g., Int) | `PatternTypeMismatch` | `variant pattern cannot match Int value` |
| `(a, b)` on 3-tuple | `PatternTupleArity` | `tuple pattern has 2 elements but tuple has 3` |
| `(a, b)` on non-tuple | `PatternTypeMismatch` | `tuple pattern cannot match Int value` |
| `"hi"` on Int | `PatternTypeMismatch` | `literal pattern of type Str cannot match Int value` |
| `Non` bare on `Opt[Int]` | `PatternAmbiguousBinding` | `'Non' is a variant of Opt[Int]; use Non() for variant match` |
| `Val(x)` in let | `PatternRefutableInLet` | `variant pattern in let binding; use match instead` |
| `42` in let | `PatternRefutableInLet` | `literal pattern in let binding; use match instead` |
| Nested: `Val((x, y))` where field is Int | `PatternTypeMismatch` | `tuple pattern cannot match Int value` (span: inner `(x, y)`) |
| `match x { }` no arms matched | `NoMatchArm` | `no arm matched value of type {subject_ty}` |
| `let (x, x) = ...` | `PatternRepeatedBinding` | `binding 'x' appears twice in pattern` |

---

## Examples

### Map cleanup

```ks
# before
let result = self._find(key)
if result._1 {
    let slot = self.slots[result._0]
    ...
}

# after
let (idx, found) = self._find(key)
if found {
    let slot = self.slots[idx]
    ...
}
```

### Nested match

```ks
match map.get(key) {
    Val((name, age)) -> print("{name} is {age}"),
    Non()            -> print("not found"),
}
```

### For-loop destructuring

```ks
for (key, val) in map {
    print("{key}: {val}")
}
```

### Wildcard sub-patterns

```ks
match slot {
    Used(_, v) -> use(v),
    Empty()    -> {},
    Del()      -> {},
}
```

### Catch-all binding

```ks
match code {
    200 -> "ok",
    404 -> "not found",
    other -> "status: {other}",
}
```

### Literal inside variant

Sub-patterns compose — a variant pattern can contain a literal sub-pattern to match against specific field values.

```ks
match result {
    Val(0)   -> "zero",
    Val(n)   -> "got {n}",
    Err(msg) -> "error: {msg}",
}
```

Resolution: `Val(0)` resolves to `Variant { type_id, variant_idx, bindings: [Literal(Value::Int(0))] }`. Matching first compares variant_idx (u32), then recurses into the field — which is a literal pattern, so it compares the field value against 0.

### Negative literal

```ks
match direction {
    1  -> "forward",
    -1 -> "backward",
    0  -> "stopped",
    _  -> "unknown",
}
```

### Nested tuple destructure

```ks
let ((a, b), c) = ((1, 2), 3)
# a = 1, b = 2, c = 3
```

---

## Validation against existing code

Every pattern shape present in the codebase (tests, stdlib, demos) was audited and verified to work under the proposed design:

| Pattern shape | Example (codebase) | Under proposal |
|---------------|-------------------|----------------|
| Data variant, 1 field | `Val(n)` | `Variant { name: "Val", bindings: [Binding("n")] }` |
| Data variant, 2 fields | `Used(k, v)`, `Both(n, s)` | `Variant { name, bindings: [Binding("k"), Binding("v")] }` |
| Unit variant | `Non`, `Empty`, `Del`, `Fish`, `Red` | **Migrate to** `Non()`, `Empty()`, etc. |
| Int literal | `200`, `404`, `0`, `1` | `Literal(Int("200"))` |
| Str literal | `"help"`, `"quit"` | `Literal(Str("help"))` |
| Wildcard | `_` | `Wildcard` |
| Empty block arm body | `Del -> {}` | unchanged — body is `Vec<Spanned<Stmt>>` (can be empty) |
| Control-flow in arm body | `Non -> ret "not found"`, `_ -> cont` | unchanged — arm bodies are statement lists |
| Match as expression | `let name = match c { ... }` | unchanged — match is already an expression |
| Match on method self | `ret match self { Val(x) -> x, ... }` | unchanged — self has a concrete TypeId at call time |
| No-match error | `match 42 { 0 -> ..., 1 -> ... }` | now returns `NoMatchArm` with the subject's TypeId |
| For-loop over Arr[T] | `for x in arr { ... }` | `For { pattern: Binding("x"), ... }` |
| For-loop over Map (tuple) | `for entry in m { entry._0 }` | replaceable with `for (k, v) in m { ... }` |
| `let` simple binding | `let x = 42`, `let result = f()` | `Let { pattern: Binding("x"), ... }` |
| `let` with array literal | `let arr = [1, 2, 3]` | unchanged — binding is irrefutable, value is any expression |
| `let` with struct literal | `let c = Cage { animal: ..., size: 3 }` | unchanged — binding is irrefutable |
| `let` shadowing | `let x = 1; let x = 2` | unchanged — each is its own `Let { pattern: Binding }` |

### Patterns not in codebase but enabled by this proposal

Several new capabilities become available. Each has been verified through the type relationships and resolution rules:

| New pattern | Example | Resolves to |
|-------------|---------|-------------|
| Tuple destructure in `let` | `let (a, b) = f()` | `Let { pattern: Tuple([Binding("a"), Binding("b")]) }` |
| Tuple destructure in `for` | `for (k, v) in map` | `For { pattern: Tuple([Binding("k"), Binding("v")]) }` |
| Tuple pattern in match | `match pair { (a, b) -> ... }` | `Tuple([Binding, Binding])` against `TupleInstance` |
| Nested tuple | `let ((a, b), c) = x` | `Tuple([Tuple([Binding, Binding]), Binding])` |
| Variant with tuple field | `Val((name, age)) -> ...` | `Variant { bindings: [Tuple([Binding, Binding])] }` |
| Variant with literal field | `Val(0) -> "zero"` | `Variant { bindings: [Literal(Int("0"))] }` |
| Wildcard inside variant | `Used(_, v) -> v` | `Variant { bindings: [Wildcard, Binding("v")] }` |
| Negative literal | `-1 -> "backward"` | `Literal(Int("-1"))` |

---

## Scope — what's deferred

| Feature | Status |
|---------|--------|
| `Spanned<String>` for pattern names (replacing bare `String`) | **this proposal** |
| Dedicated `Literal` enum (replacing `Spanned<Expr>` in patterns) | **this proposal** |
| Recursive `Pattern` with `Vec<Spanned<Pattern>>` | **this proposal** |
| `ResolvedPattern` with `TypeId + variant_idx` handles | **this proposal** |
| Two-phase match (resolve all arms, then match) | **this proposal** |
| Type-validated resolution (all arms, all depths) | **this proposal** |
| Tuple patterns in match/let/for | **this proposal** |
| Negative literal patterns (`-42`) | **this proposal** |
| Unit variant syntax `Non()` (breaking change) | **this proposal** |
| Safety-net error for bare ident matching variant name | **this proposal** |
| Structured `ErrorKind` variants for pattern errors | **this proposal** |
| Adopt `Spanned<String>` across rest of AST | follow-on |
| Refutable let (`let Val(x) = e else { ... }`) | deferred |
| Struct patterns (`{ x, y }`) | deferred |
| Guards (`if expr` after pattern) | deferred |
| Or-patterns (`A \| B`) | deferred |
| Exhaustiveness checking | deferred |

## References
- `Value::Enum { type_id, variant_idx: u32, fields }` — runtime already handle-based
- `TypeRegistry::get_variant(type_id, name) → (u32, ResolvedVariantDef)` — resolution API exists
- `EnumInstance { variants: IndexMap<String, ResolvedVariantDef> }` — variant table
- `ResolvedVariantDef { fields: Vec<TypeId> }` — field types for nested resolution
- `TupleInstance { type_args: Vec<TypeId> }` — element types for nested resolution
