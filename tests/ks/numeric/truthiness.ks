# Zero is falsy, nonzero is truthy for all numeric types
print(if U32(0) { "yes" } else { "no" })
print(if U32(1) { "yes" } else { "no" })
print(if I32(0) { "yes" } else { "no" })
print(if I32(-1) { "yes" } else { "no" })
print(if F32(0.0) { "yes" } else { "no" })
print(if F32(0.1) { "yes" } else { "no" })
