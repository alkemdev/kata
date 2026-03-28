# to_int converts fixed-width integers to BigInt
print(U32(42).to_int())
print(I8(-5).to_int())
print(U128(999).to_int())
print(typeof(U32(42).to_int()))
