# Literal sub-pattern is more specific than a binding sub-pattern.
# Both arms could match Val(0); arm order picks the literal first.
let zero = Opt[Int].Val(0)
let five = Opt[Int].Val(5)

func describe(o: Opt[Int]): Str {
    ret match o {
        Val(0) -> "zero",
        Val(x) -> "nonzero {x}",
        Non() -> "none",
    }
}

print(describe(zero))
print(describe(five))
