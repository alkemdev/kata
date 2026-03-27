# Str.len() counts codepoints, not bytes
let s = "\u2764\u2764"
print(s.len())

# substr is codepoint-indexed
let mixed = "a\u2764b"
print(mixed.len())
print(mixed.substr(1, 1))
