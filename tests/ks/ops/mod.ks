# Modulo operator — true modulo, sign follows divisor (right operand)
print(10 % 3)
print(17 % 5)
print(100 % 7)
print(0 % 5)
print(7 % 7)

# Negative: sign always follows divisor (right operand)
print(-10 % 3)
print(10 % -3)
print(-10 % -3)
print(-1 % 5)
print(1 % -5)

# Float modulo
print(10.5 % 3.0)
print(-10.5 % 3.0)

# Fixed-width modulo
print(U32(10) % U32(3))
print(I32(-10) % I32(3))
print(I32(10) % I32(-3))
