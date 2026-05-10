# b"..." interpolation coexists with escape sequences
let x = 7
let b = b"a={x}\xff\n"
print(b)
print(b.len())
