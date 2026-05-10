# Byte values are produced by indexing a Bin. The b'x' literal lexes as Bin,
# not a single Byte — so we go through indexing to get a real Byte value.
print(typeof(b"x"[0]))
