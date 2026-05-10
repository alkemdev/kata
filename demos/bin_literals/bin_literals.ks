# Byte-string literals — a tour of the `b"..."` / `b'...'` forms.
# Both produce interned `Bin` values (raw byte sequences, not Str).

# ── 1. Basic forms ───────────────────────────────────────────────
# `b"..."` interpolates `{expr}`; `b'...'` is purely literal.
# When neither has special syntax, the resulting bytes are identical.

let a = b"hello"
let b = b'hello'
print("Basic forms:")
print("  b\"hello\" = {a}")
print("  b'hello' = {b}")
print("  same content? {a == b}")
print("  len = {a.len()}")

# ── 2. Hex escapes ───────────────────────────────────────────────
# `\xNN` produces one raw byte — useful for non-printable / binary data.

let raw = b"\xff\x00\xfe"
print("\nHex escapes:")
print("  b\"\\xff\\x00\\xfe\" = {raw}")
print("  len = {raw.len()}")
print("  raw[0] = {raw[0]}")
print("  raw[1] = {raw[1]}")
print("  raw[2] = {raw[2]}")

# ── 3. Interpolation in b"..." ───────────────────────────────────
# `{expr}` is replaced with the value's display output, UTF-8 encoded.

let name = "kata"
let n = 42
let greet = b"hello {name}, n={n}"
print("\nInterpolation:")
print("  greet = {greet}")
print("  len   = {greet.len()}")

# ── 4. No interpolation in b'...' ────────────────────────────────
# Single-quoted byte strings keep `{...}` as literal text.

let x = 999
let lit = b'val={x}'
print("\nNo interp in single-quote form:")
print("  lit = {lit}")
print("  len = {lit.len()}")
print("  (literal bytes: v a l = \{ x \})")

# ── 5. Equality & interning ──────────────────────────────────────
# `==` is content equality. Identical literals share storage (Arc<[u8]>),
# so equality checks are pointer-fast.

let p = b"abc"
let q = b'abc'
print("\nEquality:")
print("  b\"abc\" == b'abc' ? {p == q}")
print("  b\"abc\" != b'xyz' ? {p != b'xyz'}")

# ── 6. ToBin protocol ────────────────────────────────────────────
# Str implements `to_bin()` natively — returns the UTF-8 byte encoding.

let s = "héllo" # 'é' is 2 UTF-8 bytes
let s_bin = s.to_bin()
print("\nToBin protocol:")
print("  \"héllo\".to_bin() = {s_bin}")
print("  Str codepoints = {s.len()}")
print("  Bin bytes      = {s_bin.len()}")

# ── 7. Display: round-trippable b'...' format ────────────────────
# Printable ASCII shows as-is; non-printable bytes use `\xNN`; common
# whitespace uses short escapes (`\n`, `\t`, etc.).

let mix = b'hi\xff\n\t!'
print("\nDisplay output:")
print("  mix = {mix}")
print("  len = {mix.len()}")

# A pure-binary blob renders entirely as escapes.
let blob = b'\x00\x01\x02\xab\xcd\xef'
print("  blob = {blob}")
print("  len  = {blob.len()}")
