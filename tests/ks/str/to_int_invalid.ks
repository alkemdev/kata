# Str.to_int on a non-numeric string is a structured ParseError.
let s = "not-a-number"
print(s.to_int())
