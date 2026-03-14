func sign(n: Int) {
    if n < 0 { ret "negative" }
    if n > 0 { ret "positive" }
    ret "zero"
}
print(sign(-1))
print(sign(0))
print(sign(1))
