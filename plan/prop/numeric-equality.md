# Decision: cross-type numeric equality
**ID:** numeric-equality
**Status:** open
**Date opened:** 2026-05-09
**Date done:** —
**Affects:** eval, stdlib, syntax (semantics of `==`)

## Question
When `==` is applied to two numeric values of different types (e.g., `I32(5) == 5`, `U8(1) == U16(1)`, `I64(1) == F64(1.0)`), what should happen?

## Context

KataScript has 17 numeric value variants: `Int` (BigInt, arbitrary precision), `Float` (f64, soon arbitrary precision per the fixed-width-numerics decision), and the 15 fixed-width prims `U8..U128`, `I8..I128`, `Usz`, `Isz`, `F16`, `F32`, `F64` defined by the `define_numeric_prims!` macro at `katars/src/ks/numeric.rs:594`.

The current equality dispatch path is:

1. `eval_binop` (`katars/src/ks/native.rs:523`) calls `numeric::try_binop` first.
2. `numeric::try_binop` (`katars/src/ks/numeric.rs:330`) only matches when **both operands are the same `Value::$V` variant** — its arm at line 339 reads `Ok(Value::Bool(a == b))`. Mixed-width pairs fall through with `None`.
3. Control returns to `eval_binop`, where line 532 evaluates `left == right || cross_eq(left, right)`. The PartialEq impl on `Value` only equates same-variant values, so the first half is always `false` for mixed pairs.
4. `cross_eq` (`katars/src/ks/native.rs:691`) handles exactly two pairs: `(Int, Float)` and `(Float, Int)`. Everything else returns `false`.

Concrete behavior today:
- `Int(5) == Int(5)` → `true` (variant equality at native.rs:532)
- `Float(5.0) == Float(5.0)` → `true` (same; note: bitwise — see task #26)
- `Int(5) == Float(5.0)` → `true` (cross_eq, native.rs:693)
- `I32(5) == Int(5)` → `false` (no cross_eq arm; both type_ids differ)
- `I32(5) == F64(5.0)` → `false`
- `U8(1) == U16(1)` → `false`
- `U8(1) == I32(1)` → `false`

So fixed-width prims form an island disconnected from cross-type equality. `Int`/`Float` are special-cased; everything else is variant-locked. This is asymmetric, and it surprises users coming from Python or JavaScript where any two numerics with the same mathematical value compare equal.

The conformance fixture `tests/ks/numeric/equality.ks` even encodes the status quo as desired: it deliberately tests only same-type equality and the file comment says "inequality across types" — though no cross-type cases are actually asserted.

This question is forced now because the fixed-width prims are new (`fixed-width-numerics.md`, decided 2026-03-28), and users hitting `let i = I32(5); if i == 5 { ... }` will get silently-false `if`s. We need to either commit to one of the alternatives or document the surprise prominently.

## Alternatives

### Option A: Universal value comparison
Any two numeric values compare by mathematical value. `I32(5) == 5 == 5.0 == U64(5) == F32(5.0)` all yield `true`. Booleans stay separate (don't conflate `true == 1`).

**Pros:**
- Matches Python/JS/Lua/Ruby — the dominant scripting-language model. Smallest surprise for the target audience.
- `5` literal works across any numeric receiver: `let i = I32(5); i == 5` is true.
- One simple rule: convert both sides to a common arithmetic value, then compare.
- Composes with collections: `[I32(1), 1, F64(1.0)].contains(1)` does what you'd expect.

**Cons:**
- Requires a careful int↔float bridge. `BigInt(2^53 + 1) == F64(2^53 + 1)` is the classic pitfall: `f64` can't represent `2^53 + 1` exactly, so naive `int_to_f64(a)? == b` returns `true` (loses precision). The current `cross_eq` at native.rs:693 has this exact bug today.
- Needs a defined comparison protocol for every (numeric, numeric) pair, not just two. With 17 variants that's 17×17 = 289 cases; a macro generalization is needed.
- Equality is no longer reflexive across reinterpretation: `F32(0.1)` and `F64(0.1)` have different bit-level values for "the same" decimal — should they be equal? (Most likely answer: compare in the wider type, so `F32 == F64` widens F32 to F64 first. This is what IEEE does.)
- Hash-equality contract: if `I32(5) == 5`, then `hash(I32(5)) == hash(5)`. The current `hash_numeric` at numeric.rs:381 hashes each variant differently, which breaks the contract. Either rewrite hashing to canonicalize or accept that only same-type values can be map keys.

### Option B: Strict typing, explicit cast
Numeric types do **not** cross-compare. `I32(5) == 5` is either a type error (preferred) or returns `false` always. Users must write `I32(5) == I32(5)` or `i as Int == 5`.

**Pros:**
- Matches Rust. No surprise from precision loss because there's no implicit conversion.
- Fastest equality dispatch — no conversion logic.
- The compiler/runtime forces the user to confront width and signedness explicitly.

**Cons:**
- Ergonomically painful for a dynamic scripting language. KataScript already has duck-typed truthiness (`truth` at native.rs:510) and dispatches operators across `Int`/`Float`. Strict equality alone would feel inconsistent.
- Existing behavior (`Int == Float` works) would have to be removed or specially carved out, breaking `tests/ks/numeric/...` and demos.
- Users can't ergonomically write `let counts: I32 = 0; if counts == 0 { ... }` — they'd need `I32(0)`. Literal `0` is `Int`, so this requires constant inference or explicit casts everywhere.
- KataScript doesn't yet have an `as` cast for prims — Token::As at lexer.rs:113 exists but is currently only used for `impl K as T` conformance. Building a numeric cast surface is a non-trivial side-quest if we go this route.

### Option C: Common-supertype rule
Define a partial order over numeric types and compare in the join:
- Two integer types: compare in `Int` (BigInt) — always exact.
- Two float types: compare in the wider float (e.g., `F16 == F64` widens both to `F64`).
- Mixed int↔float: compare in the wider float, with a precision check — if the integer is outside the float's exactly-representable range, return `false` (no false positives).
- `Int↔Float` reduces to the existing rule.

**Pros:**
- Mathematically principled: integer-vs-integer is always exact (BigInt has no width); float-vs-float widens (IEEE convention); int-vs-float guards against silent precision loss.
- Generalizes the existing `cross_eq` rather than throwing it away.
- Works well with the Int-as-default-numeric design: literal `0` is `Int`, and `I32(x) == 0` does the right thing.
- Each case is locally explicable; the user can reason about "what gets widened to what" without surprise.

**Cons:**
- The full rule has multiple branches (int-int, float-float, int-float, plus signed-vs-unsigned within ints). Specifying it precisely is more work than Option A's one-liner.
- Subtle int↔float case: should `I64::MAX == F64(I64::MAX as f64)` be true? `f64` rounds `I64::MAX = 2^63 - 1` to `2^63`. Strict reading says `false` (int has no equivalent in F64); permissive says `true` (both round-trip to same f64). Pick one and document it.
- Hash contract still applies (same as Option A).
- More code to maintain than Option A or D.

### Option D: Status quo — only Int↔Float cross-compares
Keep `cross_eq` exactly as it is (native.rs:691). Fixed-width prims never cross-compare. Document the rule prominently.

**Pros:**
- Zero implementation work.
- No risk of subtle precision bugs we haven't anticipated.
- The Int↔Float carve-out has been stable since the prim era.

**Cons:**
- Surprising: `I32(5) == 5` returns `false` while `Int(5) == 5.0` returns `true`. The asymmetry is real and inexplicable from the user's standpoint.
- The asymmetry compounds with `Int` being the default integer literal: every fixed-width prim is forever-incompatible with literal numerics.
- Likely to be fixed eventually anyway; deferring just delays the inevitable migration.
- The bug at native.rs:693 (where `Int(2^53 + 1).to_f64()` rounds and lies) remains either way.

## Discussion

**The literal-`0` problem.** KataScript integer literals are `Int`, not any fixed-width type. Once a user has a fixed-width variable, every comparison against a literal becomes cross-type:

```
let counter: I32 = 0
while counter != 100 { ... }  # status quo: infinite loop, 100 is Int
```

This makes Option D actively dangerous, not just inconsistent. Either we fix the comparison rule (A or C) or we add a path for fixed-width literal inference.

**The `2^53 + 1` problem.** `cross_eq` at native.rs:693 already has a precision bug: `to_f64()` lossily rounds, then `==` compares the rounded value to the float. So `Int(9007199254740993) == Float(9007199254740992.0)` returns `true` today. Any fix to cross-type equality should address this; Option A and Option C both naturally absorb it.

**Hash contract.** `numeric::hash_numeric` at numeric.rs:381 hashes each variant independently — `hash(Value::I32(5))` writes the variant tag plus 4 bytes; `hash(Value::Int(5))` writes a BigInt limb. So even if we make `I32(5) == Int(5)` true, they hash differently, and they can't both be keys in the same hash map. We have three options for hashing under value-equality:

1. Canonicalize numeric values at hash time: hash all integers as a normalized BigInt-ish form, all floats as their f64 bits. Pay the conversion cost on every hash.
2. Restrict hash-equality to same-variant equality (i.e., maps key on `Value` identity, not numeric value). `I32(5)` and `Int(5)` would be distinct keys even though they compare equal. This matches Rust's `HashMap<i32, V>` not accepting `i64` keys.
3. Punt: maps don't exist yet (`map-type.md` is still open), so we don't have to decide today.

Option 3 is fine for now, but the choice we make for `==` will pre-commit us in a direction. Option A pushes us toward (1); Option B/D toward (2); Option C is compatible with either.

**Boolean is not a number.** `true == 1` should be `false` regardless of which alternative we pick. Booleans are not numbers in KataScript (Bool is its own prim). All four options preserve this — none of them implicitly bridge Bool to numeric.

**Recommendation.** **Option C (common-supertype rule).** It threads the needle:
- Avoids the silent infinite-loop trap of Option D.
- Avoids the ergonomic disaster of Option B in a dynamically-typed language.
- Avoids the precision-and-hashing minefield of Option A by treating int-vs-int as exact (no float involved) and int-vs-float as guarded.

The rule generalizes one already-shipped behavior (`Int == Float`), so users who learned the existing model don't unlearn anything. The implementation is one new function (`numeric::cross_eq_numeric`) plus a deletion of the special case at native.rs:691. The float-precision guard is the same logic users would have to apply mentally anyway.

The main concession: spelling out the rule in the spec is several paragraphs, not a sentence. We pay in spec verbosity to gain in runtime predictability.

## Decision
TBD — pending review

## Implementation sketch

Approximate scope: ~80 lines of code, ~40 lines of tests. Mechanical, no unknowns once the rule is fixed.

**File changes:**
- `katars/src/ks/numeric.rs` — add `pub fn cross_eq_numeric(a: &Value, b: &Value) -> bool` covering int-int, float-float, and int-float cases. Use the existing `NumericFloat::to_f64` helper at numeric.rs:32 for float widening; convert fixed-width ints to `BigInt` for int-int comparison (avoid signed/unsigned fence-post). For int↔float, convert int to the float's width via `to_f64`/`to_f32`/`to_f16` and verify the round-trip back to int matches the original (precision guard).
- `katars/src/ks/native.rs:691` — replace the existing `cross_eq` with a thin wrapper: `numeric::cross_eq_numeric(left, right)`. Delete the `(Int, Float)` / `(Float, Int)` arms (now subsumed). Lines 532–533 unchanged.
- `katars/src/ks/numeric.rs:381` — leave `hash_numeric` as-is for now; defer the hash-equality contract to the `map-type` proposal. Add a comment marker so the reviewer knows it's a known gap.

**Tests** (`tests/ks/numeric/cross_equality.ks` + `.expected`):
- `I32(5) == 5` → `true`
- `5 == I32(5)` → `true` (symmetric)
- `U8(1) == U16(1)` → `true`
- `I32(-1) == U32(1)` → `false` (signed/unsigned, different mathematical values)
- `I64(1) == F64(1.0)` → `true`
- `Int(9007199254740993) == F64(9007199254740992.0)` → `false` (precision guard; this fixes the latent bug)
- `F32(0.1) == F64(0.1)` → `false` (F32(0.1) widened to F64 ≠ F64(0.1) — IEEE rounding differs)
- `true == 1` → `false` (Bool is not numeric)
- `I32(5) != I32(5)` for the inverse path — sanity that `!=` stays consistent

**Update** the existing `tests/ks/numeric/equality.ks` comment, which currently says "inequality across types" — that's now wrong under Option C.

## References
- [spec: fixed-width-numerics](../../docs/spec/fixed-width-numerics.md) — establishes the 15 prim types and the no-implicit-promotion rule for arithmetic. This proposal is consistent: arithmetic still requires explicit conversion; equality is the carve-out.
- `katars/src/ks/native.rs:691` — current `cross_eq`
- `katars/src/ks/native.rs:532` — equality dispatch in `eval_binop`
- `katars/src/ks/numeric.rs:330` — same-variant `try_binop` path
- `katars/src/ks/numeric.rs:594` — `define_numeric_prims!` invocation, full type list
- Python data model: `__eq__` with numeric tower (int, float, complex all cross-compare)
- Rust: `==` requires `PartialEq<Rhs>`; `i32 == i64` is a type error
- IEEE 754 §5.11: comparison of floats of different widths uses the wider format
- Task #26 — `Float == uses IEEE semantics` — interacts with this proposal; both must settle before equality is "done"
