# Bitwise methods on Byte
print(Byte(15).band(Byte(255)))
print(Byte(0).ior(Byte(1)))
print(Byte(255).xor(Byte(15)))
print(Byte(255).inv())
print(Byte(1).shl(4))
print(Byte(128).shr(7))
print(Byte(42).to_int())
