# Str len vs char_len on multi-byte UTF-8
let s = "\u2764\u2764"
print(s.char_len())
print(s.len())
