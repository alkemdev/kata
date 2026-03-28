# Decision: Fixed-width numeric types
**ID:** fixed-width-numerics
**Status:** done
**Date opened:** 2026-03-28
**Date done:** 2026-03-28
**Affects:** interpreter, stdlib, lexer

## Question
What fixed-width numeric primitive types should KataScript support?

## Context
Currently: Int (BigInt), Float (f64), Byte (u8, bitwise only). For hash tables, memory indexing, and interop with real systems, we need fixed-width integers and floats. Byte remains a distinct type (bitwise ops, no arithmetic).

## The type set

### Unsigned integers
U8, U16, U32, U64, U128

### Signed integers
I8, I16, I32, I64, I128

### Pointer-width integers
Usz (usize), Isz (isize) — platform-dependent width (64-bit on 64-bit systems)

### Floats
F16, F32, F64, F128

### Existing types (unchanged)
- Int — arbitrary-precision integer (BigInt)
- Float — alias for F64? or stays as its own thing?
- Byte — unsigned 8-bit, bitwise ops only, no arithmetic

## Design decisions

### Float relationship
**Option A:** Float becomes an alias for F64. One type, two names.
**Option B:** Float stays distinct. F64 is a separate prim. Float is "the default float", F64 is "explicitly 64-bit."

### Rust representation
Two Value variants that cover all widths:
```rust
Value::UInt { width: NumWidth, val: u128 }  // U8..U128, Usz
Value::SInt { width: NumWidth, val: i128 }  // I8..I128, Isz
Value::FInt { width: NumWidth, val: ???  }  // F16..F128
```

Width enum: `W8, W16, W32, W64, W128, Wsize`

For floats: F16 uses `half::f16`, F32 uses `f32`, F64 uses `f64`, F128 uses `f128` (nightly) or a soft-float library. All stored as bits in a u128 for uniform representation.

### Literal syntax
Constructor style: `U32(42)`, `F32(3.14)`, `I8(-5)`
Consistent with existing `Byte(0xff)`.

### Arithmetic
- Operations within same type only: `U8 + U8 -> U8`, no implicit promotion
- Cross-type: must cast explicitly via Make/From
- Overflow: checked by default (panic). Wrapping methods later (.wrapping_add, etc.)

### Bitwise ops
All integer types get bitwise ops (band, ior, xor, inv, shl, shr) — same as Byte has now.

### Byte stays separate
Byte is for raw data (network buffers, binary formats). It has bitwise ops but no arithmetic. U8 is a number that happens to be 8 bits. Different semantics, different types.

## Implementation order
1. Add NumWidth enum and Value::UInt/SInt/FInt variants
2. Register all types in TypeRegistry as prims
3. Arithmetic ops (native handlers, checked)
4. Bitwise ops for integer types
5. Comparison ops
6. Display
7. Conversion protocols (Make/From) — separate proposal
8. Conformance tests

## Future
- U256, I256 — when needed (crypto, etc.)
- Wrapping/saturating arithmetic methods
- SIMD-friendly vector types

## Decision
**Chosen:** 15 distinct prim types (U8-U128, I8-I128, Usz, Isz, F16, F32, F64). F128 deferred.
**Rationale:** Each type gets its own Value variant. A macro in `numeric.rs` generates all repetitive code. Byte and Float remain distinct. Checked overflow for integers, IEEE semantics for floats.
**Consequences:**
- 15 new prim types with arithmetic, comparison, bitwise (ints), and constructor support
- F128 deferred until Rust stabilizes f128
- Float stays separate (will become arbitrary-precision)
- Byte stays separate (bitwise only, no arithmetic)

## References
- Rust: u8..u128, i8..i128, usize, isize, f32, f64
- Zig: u8..u128, i8..i128, f16, f32, f64, f80, f128
- Current Byte impl in native.rs
