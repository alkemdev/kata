# `s.chars()` produces Arr[Char] — Char is reachable from KS source
# through the iterator protocol, even though there's no char literal.
let s = "héllo"
let cs = s.chars()

# Char count matches codepoint count (héllo is 5 codepoints, 6 UTF-8 bytes).
print(cs.len)

# Iterating produces Char values.
for c in cs {
    print("{c} -> {c.to_int()}")
}

# Random access via Arr indexing.
print(cs[0].is_alpha())
print(cs[0].to_upper())

# typeof confirms the element type.
print(typeof(cs[0]))
print(typeof(cs))
