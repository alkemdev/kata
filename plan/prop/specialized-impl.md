# Proposal: Specialized impl blocks with `@` binding
**ID:** specialized-impl
**Status:** open
**Date opened:** 2026-03-26
**Affects:** lexer, parser, AST, interpreter (register_impl, lookup_method, conformance), type solver

## Problem

Today `impl` only accepts bare names with type parameter *declarations*:

```
impl Arr[T] {           # T is a fresh parameter — methods apply to all Arr[X]
    func len(self): Int { ... }
}
```

You cannot write a *specialized* impl that targets a specific instantiation:

```
impl Arr[Byte] as ToBin {    # Byte is concrete — methods only for Arr[Byte]
    func to_bin(self): Bin { ... }
}
```

This is because `Stmt::Impl` stores `type_name: String` (a bare ident) and `type_params: Vec<String>` (parameter declarations). The parser grabs `Ident` then `[Ident, ...]` — it can't distinguish `[T]` (declare param) from `[Byte]` (apply concrete type).

Without this, protocols like `ToBin` can't be implemented for specific generic instantiations, and any per-instantiation behavior requires hacky native-side special-casing.

## Design

### The `@` binding sigil

The `@` sigil marks type pattern variables — names bound during type pattern matching. It means "bind this name to whatever type occupies this position."

```
impl Arr[@T] { ... }                    # @T binds — generic over all Arr
impl Arr[Byte] as ToBin { ... }         # Byte is concrete — specialized
impl Arr[Opt[@U]] as Flatten { ... }    # @U binds, Opt is concrete — partial
impl Pair[@T, @T] as Eq { ... }         # @T binds, repeated — must unify
impl Map[@K, Arr[@K]] as SelfIndexed    # @K binds, structural constraint
```

**Binding site vs use site:** `@T` in the impl head is the *binding site*. Inside method signatures and bodies, the bound name is used bare:

```
impl Arr[Opt[@U]] as Flatten {
    func flatten(self): Arr[U] { ... }   # U without @ — already in scope
}
```

**Why `@`:**
- Reads as "at this position, bind" — the Haskell as-pattern parallel (`x@(Just y)`)
- Visually distinct without being noisy; holds up with repeated variables (`Pair[@T, @T]`)
- Generalizes to a binding operator in other contexts (match patterns, destructuring)
- Decorators (`@derive`) occupy a different syntactic position (statement-level), no ambiguity

### Syntax overview

```
# Fully generic — @T binds, all Arr instantiations get these methods
impl Arr[@T] {
    func len(self): Int { ... }
    func push(self, val: T) { ... }      # T used bare in methods
}

# Fully specialized — no bindings, only Arr[Byte]
impl Arr[Byte] as ToBin {
    func to_bin(self): Bin { ... }
}

# Partially specialized — @U binds, but Opt is concrete structure
impl Arr[Opt[@U]] as Flatten {
    func flatten(self): Arr[U] { ... }
}

# Repeated binding — both positions must be the same type
impl Pair[@T, @T] as Eq {
    func eq(self, other: Pair[T, T]): Bool { ... }
}

# Non-generic (unchanged)
impl Int {
    func abs(self): Int { ... }
}
```

### Lexer change

Add `@` as a token. In the context of type patterns within `impl` heads, `@Ident` is parsed as a binding. No changes needed elsewhere — `@` at statement level can later be used for decorators.

### AST change

```rust
// New: a type pattern node — distinguishes bound variables from concrete types
enum TypePattern {
    /// A concrete type: `Int`, `Byte`
    Concrete(Spanned<Expr>),
    /// A bound type variable: `@T`
    Binding(String, Span),
    /// A generic application with sub-patterns: `Opt[@U]`, `Arr[Byte]`
    Apply {
        base: Spanned<Expr>,          // the base type name
        args: Vec<Spanned<TypePattern>>,
    },
}

// Before
Stmt::Impl {
    type_name: String,
    type_params: Vec<String>,
    as_type: Option<Spanned<Expr>>,
    methods: Vec<Spanned<FuncDef>>,
}

// After
Stmt::Impl {
    target: Spanned<TypePattern>,
    as_type: Option<Spanned<Expr>>,
    methods: Vec<Spanned<FuncDef>>,
}
```

`type_params` is gone — the bound variables are extracted from the `TypePattern` by the interpreter. The pattern itself carries all the information.

### Type solver

The impl system needs a generalized constraint solver to handle pattern matching, unification, and (eventually) trait bounds. This should be a distinct, inspectable module — not ad-hoc logic scattered through the interpreter.

#### Core types

```rust
/// A constraint generated during type pattern analysis.
#[derive(Debug, Clone)]
enum TypeConstraint {
    /// A binding variable must equal a concrete type.
    /// Generated when unifying @T against a known TypeId.
    Eq(String, TypeId),
    /// Two binding variables must be the same type.
    /// Generated from repeated @T in a pattern.
    Unify(String, String),
    /// A type must structurally match a pattern.
    /// e.g., the arg at position 0 must be Opt[_].
    Structure {
        position: usize,
        base: TypeId,
        sub_patterns: Vec<TypePattern>,
    },
}

/// Result of solving constraints for one impl candidate.
#[derive(Debug, Clone)]
struct SolveResult {
    /// Bindings: variable name → resolved TypeId.
    bindings: IndexMap<String, TypeId>,
    /// Specificity score: number of concrete nodes in the pattern.
    /// Higher = more specific. Used to rank candidates.
    specificity: usize,
}

/// Solver outcome — the full picture for debugging/inspection.
#[derive(Debug)]
struct SolveOutcome {
    /// All candidates considered, with their match status.
    candidates: Vec<CandidateResult>,
    /// The winning candidate, if any.
    winner: Option<usize>,
}

#[derive(Debug)]
struct CandidateResult {
    pattern: ImplPattern,
    result: Result<SolveResult, SolveError>,
}

#[derive(Debug)]
enum SolveError {
    /// A concrete type didn't match the expected type.
    TypeMismatch { position: usize, expected: TypeId, actual: TypeId },
    /// A binding variable was required to be two different types.
    ConflictingBinding { variable: String, first: TypeId, second: TypeId },
    /// Structural mismatch: expected a generic application, got a prim.
    NotGeneric { position: usize, expected_base: TypeId, actual: TypeId },
}
```

#### Unification algorithm

```
unify(pattern: TypePattern, concrete: TypeId, bindings: &mut Map) -> Result<(), SolveError>:

    match pattern:
        Concrete(expr):
            resolve expr to TypeId
            if resolved != concrete: return TypeMismatch
            OK

        Binding(name):
            if name in bindings:
                if bindings[name] != concrete: return ConflictingBinding
            else:
                bindings[name] = concrete
            OK

        Apply { base, args }:
            resolve base to base_id
            if base_type(concrete) != base_id: return NotGeneric
            concrete_args = instance_type_args(concrete)
            if args.len() != concrete_args.len(): return ArityMismatch
            for (sub_pattern, concrete_arg) in zip(args, concrete_args):
                unify(sub_pattern, concrete_arg, bindings)?
            OK
```

This is standard first-order unification — walk both structures in lockstep, binding variables as you go, erroring on conflicts.

#### Specificity

Given multiple matching candidates, pick the most specific one:

```
specificity(pattern) -> usize:
    match pattern:
        Concrete(_): 1
        Binding(_): 0
        Apply { base, args }: 1 + sum(specificity(arg) for arg in args)
```

More concrete nodes = more specific:
- `Arr[@T]`: specificity 1 (just Arr)
- `Arr[Opt[@U]]`: specificity 2 (Arr + Opt)
- `Arr[Opt[Int]]`: specificity 3 (Arr + Opt + Int)
- `Arr[Byte]`: specificity 2 (Arr + Byte)

Ties at the same specificity are an error (ambiguous impl).

### Dispatch algorithm

```
lookup_method(type_id, name):
    # Step 1: Exact match on the concrete instance TypeId.
    if methods[type_id] has name:
        return it (with empty bindings — no params to resolve)

    # Step 2: Pattern matching against partially-specialized impls.
    base = base_type(type_id)
    concrete_args = instance_type_args(type_id)
    candidates = []
    for (pattern, method_table) in impl_patterns[base]:
        if method_table has name:
            match unify(pattern, type_id):
                Ok(solve_result) => candidates.push(solve_result, method_table)
                Err(_) => skip

    if candidates.len() > 1:
        sort by specificity, error if top two tie
    if candidates.len() >= 1:
        return method + winner's bindings (used as instance_type_args)

    # Step 3: Base type fallback (fully generic impl).
    if methods[base] has name:
        return it (with instance_type_args from the value's type)

    # Step 4: Not found.
    None
```

#### Inspectability

The solver should be queryable for debugging and development. Potential introspection surfaces:

- **`--dump-dispatch`** flag or built-in: given a type and method name, print the full `SolveOutcome` — all candidates considered, which matched, which won, why others were rejected.
- **`typeof(x).impls`** or similar introspection from KataScript: list all impl patterns that match a given type.
- **Error messages** that show the solver's work: "method `to_bin` not found on `Arr[Int]`. Candidates considered: `impl Arr[Byte] as ToBin` — rejected: TypeMismatch at position 0 (expected Byte, got Int)."
- All solver types derive `Debug` — `SolveOutcome`, `CandidateResult`, `SolveError` are all printable.

### Conformance

Today conformance stores `(type_base, iface_base)`. With specialization, it needs the same three-tier structure:

```rust
/// Fully concrete conformance: (Arr[Byte], ToBin)
concrete_conformances: HashSet<(TypeId, TypeId)>,

/// Pattern-based conformance: (Arr, pattern, ToBin)
/// For checking "does Arr[Opt[Int]] conform to Flatten?"
pattern_conformances: Vec<(ImplPattern, TypeId)>,

/// Base-level conformance: (Arr, ToIter) — as today
base_conformances: HashSet<(TypeId, TypeId)>,
```

`conforms_to(concrete, interface)` checks all three tiers in specificity order.

### Storage

```rust
/// Interpreter fields:
pub struct Interpreter {
    // ...existing...

    /// Methods on exact TypeIds (concrete instances + base types).
    /// This is the existing `methods` field — no change.
    methods: HashMap<TypeId, IndexMap<String, Value>>,

    /// Partially-specialized impl patterns, grouped by base type.
    /// For each base type, a list of (pattern, method_table) pairs.
    impl_patterns: HashMap<TypeId, Vec<(ImplPattern, IndexMap<String, Value>)>>,
}

/// A type pattern from an impl head, with bindings extracted.
#[derive(Debug, Clone)]
struct ImplPattern {
    /// The original TypePattern from the AST (for error messages / inspection).
    pattern: TypePattern,
    /// Names of bound variables, in order. ["T"] or ["K", "V"].
    bindings: Vec<String>,
    /// Precomputed specificity score.
    specificity: usize,
}
```

### register_impl flow

1. **Parse the `TypePattern`** from the impl head.
2. **Extract bindings:** walk the pattern, collect all `@Name` bindings. Error on duplicate names in non-unifying positions (details TBD).
3. **Classify the pattern:**
   - No bindings → fully concrete. Resolve the full type expression to a TypeId. Store methods in `methods[instance_type_id]`.
   - All top-level args are bindings with no structure → fully generic (equivalent to today). Store methods in `methods[base_type_id]`.
   - Mixed → partially specialized. Build `ImplPattern`, store in `impl_patterns[base_type_id]`.
4. **Resolve method params:** same as today, but `type_params` comes from the extracted binding names instead of the AST field.
5. **Register conformance** if `as_type` is present, in the appropriate tier.

### Future extensions

The constraint solver is designed to grow:

- **Trait bounds:** `impl Arr[@T: ToStr] as ToStr` — adds a `Conforms(T, ToStr)` constraint to the solver. Unification succeeds only if the bound is satisfiable.
- **Where clauses:** `impl Map[@K, @V] as ToStr where K: ToStr, V: ToStr` — multiple constraints on multiple variables.
- **Negative constraints:** `impl Arr[@T] as Default where T: !Drop` — exclude types with certain properties.
- **Associated types:** constraint solver binds not just type variables but associated type outputs.

Each of these adds new `TypeConstraint` variants and solver rules, but the core unification + specificity framework stays the same.

## Phasing

**Phase 1 — full specialization + unification infrastructure:**
- Lexer: add `@` token
- Parser: parse `TypePattern` in impl heads
- AST: `Stmt::Impl` uses `TypePattern` target
- Type solver module: `TypeConstraint`, `SolveResult`, `SolveOutcome`, `unify()`
- Interpreter: `register_impl` classifies patterns, stores in appropriate tier
- Dispatch: exact match + base fallback (existing), handles fully concrete specialization
- Conformance: three-tier storage and query
- Inspectability: `Debug` on all solver types, descriptive error messages

**Phase 2 — partial specialization dispatch:**
- Add `impl_patterns` storage
- Dispatch step 2: pattern matching candidates via `unify()`
- Specificity ordering and ambiguity detection
- `--dump-dispatch` or equivalent inspection tool

**Phase 3 — trait bounds:**
- Add `Conforms` constraint variant
- Solver checks trait satisfaction during unification
- Where-clause parsing and constraint generation

Phase 1 unblocks `impl Arr[Byte] as ToBin` and builds the solver infrastructure. Phase 2 enables partial specialization. Phase 3 enables conditional impls.

## Alternatives

### A: Name resolution instead of `@` sigil
Distinguish params from concrete types by whether the name resolves in scope.
**Rejected:** Typos silently create type parameters instead of erroring. `impl Arr[Byt]` becomes generic instead of "unknown type Byt". The `@` sigil makes intent explicit.

### B: Leading parameter declaration (Rust-style `impl[T] Arr[T]`)
Declare params before the target.
**Rejected:** Workable but redundant — T appears twice. Doesn't compose as cleanly with partial specialization (`impl[U] Arr[Opt[U]]` is noisier than `impl Arr[Opt[@U]]`). The `@` sigil carries the same information with less ceremony.

### C: Constraint syntax (`impl Arr[T: Byte]`)
Params with equality constraints.
**Rejected:** Collides with future trait bounds (`T: ToStr`). Would need a separate sigil for equality vs conformance constraints. The pattern-based approach avoids this by not overloading `:`.

### D: Native-only specialization
Register specialized impls in Rust at bootstrap.
**Rejected:** Hacky, doesn't generalize, can't be used from KataScript.

## Decision
`@` sigil with pattern-based impl targets. Three-phase rollout: full specialization → partial specialization → trait bounds.

## References
- Rust: `impl<T> Trait for Vec<T>` (generic), `impl Trait for Vec<u8>` (specialized), specialization RFC (unstable)
- Haskell: instance resolution with most-specific match, overlapping instances, type class constraints
- Swift: constrained extensions (`extension Array where Element == UInt8`)
- Scala: implicit specialization with type bounds
