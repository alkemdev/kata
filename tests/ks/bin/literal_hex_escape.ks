# \xff in byte strings produces a single raw byte
let b = b'\xff\x00\xab'
print(b.len())
print(b[0])
print(b[1])
print(b[2])
