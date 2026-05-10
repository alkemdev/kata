# b'...' does NOT interpolate — {x} stays literal text
let x = 42
let b = b'val={x}'
print(b)
print(b.len())
