# Literal sub-pattern inside a variant pattern: Val(0)
let zero = Opt[Int].Val(0)
let one = Opt[Int].Val(1)
let none = Opt[Int].Non

print(match zero { Val(0) -> "zero", Val(n) -> "got {n}", Non() -> "none" })
print(match one  { Val(0) -> "zero", Val(n) -> "got {n}", Non() -> "none" })
print(match none { Val(0) -> "zero", Val(n) -> "got {n}", Non() -> "none" })
