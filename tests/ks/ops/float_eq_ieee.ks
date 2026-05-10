# IEEE float equality: -0.0 == 0.0 is true (a sign-bit, not a value).
# NaN != NaN — but that's covered as comparison error, not a == arm.
print(-0.0 == 0.0)
print(0.0 == -0.0)
let zero = 0.0
let neg = -0.0
print(zero == neg)
