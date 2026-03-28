# Bitwise operations on fixed-width integers
print(U8(0xff).band(U8(0x0f)))
print(U8(0xf0).ior(U8(0x0f)))
print(U8(0xff).xor(U8(0x0f)))
print(U8(0x0f).inv())
print(U32(1).shl(4))
print(U32(16).shr(2))
