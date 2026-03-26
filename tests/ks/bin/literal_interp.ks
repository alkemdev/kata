# b"..." with interpolation — values are display()'d and UTF-8 encoded
let x = 42
let b = b"val={x}"
print(b.len())
# "val=42" is 6 ASCII bytes
print(b[0])
print(b[4])
print(b[5])
