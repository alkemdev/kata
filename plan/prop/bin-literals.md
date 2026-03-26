# Proposal: Bin literals, byte string syntax, and Bin display
**ID:** bin-literals
**Status:** open
**Date opened:** 2026-03-26
**Affects:** lexer, parser, interpreter, value display, std.core

## Problem

Bin values exist but construction is verbose — build an Arr[Byte], push each byte, call `.to_bin()`. There's no literal syntax for binary data, no way to express arbitrary byte sequences concisely, and Bin display (`<bin:N bytes>`) is opaque and not round-trippable.

## Design

### Byte string literals: `b"..."` and `b'...'`

Prefix `b` on a string literal produces a `Bin` instead of a `Str`. The source text is UTF-8 encoded into bytes. Escape sequences produce raw bytes.

```
b"hello"              # Bin of UTF-8 bytes [0x68, 0x65, 0x6c, 0x6c, 0x6f]
b'\xff\x00\xab'       # arbitrary bytes via hex escapes
b"header: {val}\r\n"  # interpolation: val is display→UTF-8 encoded
b""                    # empty Bin (interned singleton)
```

- `b"..."` supports `{expr}` interpolation (same as `"..."`).
- `b'...'` is literal only, no interpolation (same as `'...'`).
- If there is a space between `b` and the quote (`b "hello"`), `b` lexes as `Ident("b")` and the string is a normal Str. No ambiguity.

### Lexer

The lexer sees `b"` or `b'` and enters byte-string scanning mode. Reuses the existing string scanner (`scan_string_body`) but produces a different token:

```rust
Token::BinLit(Vec<u8>)              // b'...' — no interpolation
Token::BinInterp(Vec<BinPart>)      // b"..." — with interpolation parts

enum BinPart {
    Bytes(Vec<u8>),                  // literal bytes from text + escapes
    Interp(String, usize),           // {expr} — to be evaluated and UTF-8 encoded
}
```

The lexer callback for `b"` / `b'`:
1. Scan the string body as usual (same escape processing, same interpolation detection).
2. Convert literal text segments to UTF-8 bytes immediately.
3. `\xNN` escapes produce a single byte.
4. `\u{NNNN}` escapes produce UTF-8 bytes of the codepoint.
5. Named escapes (`\n`, `\t`, etc.) produce their ASCII byte values.
6. Return `BinLit` (no interpolation) or `BinInterp` (has interpolation parts).

### Escape sequences (all string types)

Extend the string scanner with two new escapes, available in all string types (`"..."`, `'...'`, `b"..."`, `b'...'`):

| Escape | Meaning | Bytes produced |
|--------|---------|---------------|
| `\xNN` | Hex byte (exactly 2 hex digits) | 1 byte: `NN` |
| `\uNNNN` | Unicode codepoint (exactly 4 hex digits, BMP) | UTF-8 encoding of the codepoint |
| `\UNNNNNNNN` | Unicode codepoint (exactly 8 hex digits, full range) | UTF-8 encoding of the codepoint |
| `\0` | Null byte | 1 byte: `0x00` |
| `\r` | Carriage return | 1 byte: `0x0D` |

Existing escapes unchanged: `\n`, `\t`, `\\`, `\"`, `\'`, `\{`, `\}`.

Note: `\u` and `\U` use fixed-width hex (no braces) to avoid ambiguity with `{expr}` interpolation in double-quoted strings.

In regular strings (`"..."`, `'...'`), `\xNN` produces the character at that byte value (must be valid UTF-8 in context). `\u{NNNN}` produces a Unicode character. In byte strings (`b"..."`, `b'...'`), both produce raw bytes.

### Interpolation in byte strings

`b"status: {code}\n"` — `code` is evaluated, its display representation is UTF-8 encoded into bytes, and spliced into the Bin.

For now, interpolated values use the same display path as regular string interpolation (the value's `.display()` output, UTF-8 encoded). Future: prefer `to_bin()` if the value's type conforms to `ToBin`, fall back to display→UTF-8 otherwise.

### Interpreter

Byte string literals go through the intern table:
- `b'...'` (no interpolation): lexer produces `Vec<u8>`, interpreter calls `intern_bin(bytes)`.
- `b"..."` (interpolation): interpreter evaluates interpolated parts, encodes to UTF-8, concatenates all byte segments, then calls `intern_bin(result)`.

### Bin display

Bin values display as byte string literals. The output is a valid `b'...'` literal that round-trips:

```
b'hello'                          # all printable ASCII
b'hello\xff\n'                    # mixed printable + escapes
b'\xff\x00\xab'                   # no printable bytes
b''                               # empty
```

Rules:
- ASCII 0x20..0x7E (space through tilde) display as literal characters, **except** `\`, `'`, which are escaped.
- `\n` (0x0A), `\t` (0x09), `\0` (0x00) use named escapes.
- All other bytes use `\xNN`.
- Display always uses single-quote form (`b'...'`) since the output has no interpolation.

### Static methods on types

`Bin.from_base64("...")` is a static method — called on the type value, no `self` parameter. This is the first static method in KataScript.

Today `register_impl` requires the first parameter to be `self`. For static methods, relax this: if a method is called on a `Value::Type(tid)`, look up the method on `tid` and call it without prepending a receiver.

```
impl Bin {
    # Regular method — has self
    func len(self): Int { ... }

    # Static method — no self
    func from_base64(s: Str): Bin { ... }
}
```

At the call site:
- `some_bin.len()` — receiver is a Bin value, dispatch as today.
- `Bin.from_base64("...")` — receiver is `Value::Type(prim::BIN)`, look up `from_base64` on Bin's method table, call without prepending receiver.

The parser and AST don't change — method definitions already allow any parameter list. Only `register_impl` (which currently errors on missing `self`) and `eval_call` (which currently always prepends receiver) need adjustment.

### `Bin.from_base64(str) -> Bin`

Native static method on Bin. Decodes a base64-encoded string into bytes, interns the result.

```
let b = Bin.from_base64("SGVsbG8=")
print(b)                # b'Hello'
```

Invalid base64 input is a runtime error.

## Phasing

**Phase 1 — escape sequences:**
- Add `\xNN`, `\u{NNNN}`, `\0` to `scan_string_body`
- Works for all string types immediately
- Tests

**Phase 2 — byte string literals:**
- Lexer: `b"` / `b'` tokens
- Parser: `Expr::BinLit` and `Expr::BinInterp`
- Interpreter: evaluate and intern
- Tests

**Phase 3 — Bin display:**
- Update `Value::display()` and `Display for Value` to produce `b'...'` format
- Update affected test expectations
- Tests

**Phase 4 — static methods:**
- Relax `self` requirement in `register_impl`
- Handle `Value::Type` receiver in method dispatch
- Tests

**Phase 5 — `Bin.from_base64()`:**
- Native static method, base64 decode, intern
- Consider: add `base64` as a dependency or hand-roll? (It's ~50 lines for decode-only.)
- Tests

## Alternatives

### Hex literal syntax (`0x[ff00ab]`)
A separate hex-byte literal instead of byte strings.
**Deferred:** Byte strings are more general (handle text + arbitrary bytes), and the `b"..."` convention is widely understood. Hex literal could complement byte strings later for non-UTF-8 heavy use cases.

### Separate `from_base64` function instead of static method
`base64_decode("...")` as a standalone or module function.
**Rejected:** Static methods are a natural fit and generally useful beyond just Bin. Worth adding the machinery now.

## Decision
Proceed with phased implementation as described.

## References
- Python: `b"hello"`, `b'\xff'`, `base64.b64decode()`
- Rust: `b"hello"`, `b'\xff'`, `base64` crate
- Go: `[]byte("hello")`, `encoding/base64`
