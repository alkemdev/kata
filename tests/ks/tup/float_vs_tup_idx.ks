# Float literals and tuple indexing share `.` syntax — disambiguated
# purely by parser context (atom position vs postfix position).
let pi = 3.14
print(pi)

# Tuple indexing on a tuple of floats
let t = (1.5, 2.5, 3.5)
print(t.0)
print(t.1)
print(t.2)

# Nested tuple-of-tuples: `t.0.1` lexes as Ident Dot Num Dot Num,
# parsed as two postfix ops (no string-splitting in the parser).
let nested = ((10, 20), (30, 40))
print(nested.0.0)
print(nested.0.1)
print(nested.1.0)
print(nested.1.1)
