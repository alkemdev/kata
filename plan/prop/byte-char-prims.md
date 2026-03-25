# Decision: Byte and Char primitive types
**ID:** byte-char-prims
**Status:** open
**Date opened:** 2026-03-25
**Date done:** —
**Affects:** lexer, parser, interpreter, types, stdlib

## Question
How should Byte (8-bit) and Char (Unicode codepoint) primitives work?

## Design

### Byte
- Storage: `Value::Byte(u8)`
- TypeId: `prim::BYTE`
- Semantics: a bag of bits, NOT a number. No arithmetic (+, -, *, /).
- Operations: bitwise AND, OR, XOR, NOT, shift left, shift right
- Conversions: `Byte.to_int(): Int`, `Int.to_byte(): Byte` (panics if out of 0-255)
- Literals: `0xFF`, `0b10101010` (hex/binary literals produce Byte)
- Display: `0xFF` (hex by default)

### Char
- Storage: `Value::Char(char)` (Rust char = 32-bit Unicode scalar value)
- TypeId: `prim::CHAR`
- Semantics: a character, NOT a number. No arithmetic.
- Operations: comparison (==, !=, <, >, <=, >=), classification methods
- Conversions: `Char.to_int(): Int`, `Int.to_char(): Char` (panics if invalid codepoint)
- Literals: TBD — backtick? `\`a\``? Or just `Char.from("a")`?
- Methods (via `impl Char`): `is_alpha`, `is_digit`, `is_upper`, `is_lower`, `to_upper`, `to_lower`
- Display: the character itself

### Bitwise operators (new)
- `&` AND, `|` OR, `^` XOR — binary operators on Byte
- `~` NOT — unary operator on Byte
- `<<` shift left, `>>` shift right — Byte << Int → Byte
- These are NEW tokens and operators, only valid on Byte

### Relationship to Str and Bin
- `Str.chars()` → iterator of Char
- `Str.bytes()` → iterator of Byte
- `Str.len()` → Int (number of bytes)
- `Str.char_len()` → Int (number of codepoints)
- `Bin` stays as `Value::Bin(Vec<u8>)` — a sequence of Byte values
- `Bin.len()` → Int
- `Bin[i]` → Byte (via GetItem)

### Open questions
- Byte literals: should `0xFF` produce Byte? Or Int? Rust uses `0xFFu8` suffix. We could use `b'0xFF'` or just `0xFF`.
- Char literals: backtick? Or single-char single-quoted string auto-coerces?
- Should `&` `|` also work on Bool? (AND/OR are `&&`/`||` for short-circuit, but bitwise on bools is reasonable)
- Should Int support bitwise ops too? Or only Byte?

## Alternatives

### Option A: Byte and Char as prims (proposed above)
**Pros:** Clean type safety, clear semantics, enables proper Str methods.
**Cons:** Two new Value variants, new operators, literal syntax questions.

### Option B: Just use Int for both
**Pros:** No new types. Simple.
**Cons:** No type safety. `arr_of_bytes[0] + 1` silently works. A "character" is just an int with no methods.

### Option C: Byte/Char as kinds wrapping Int
`kind Byte { val: Int }`, `kind Char { val: Int }` in KS.
**Pros:** No new prims. Self-hosted.
**Cons:** Wrapping overhead. Every byte/char operation unwraps and rewraps. Constructor needs validation. Bitwise ops need method syntax not operator syntax.

## Decision
<!-- blank while open -->

## References
- Rust: `u8`, `char` (4 bytes, Unicode scalar value)
- Go: `byte` (alias for uint8), `rune` (alias for int32)
- Python: no separate byte/char types (bytes is a sequence, str is Unicode)
- Current prims: Nil, Bool, Int, Float, Str, Bin, Func, Type, RawPtr
