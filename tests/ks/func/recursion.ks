# A function can call itself by name from inside its body.
func fact(n: Int): Int {
    if n == 0 { ret 1 }
    ret n * fact(n - 1)
}
print(fact(0))
print(fact(1))
print(fact(5))
print(fact(10))
