# Forward references work too: `is_even` can call `is_odd` which is
# defined later because both names are resolved at call time, not at
# definition time. The body's captured scope is mutable through shared
# slots — `is_odd` becomes visible once it's bound below.
func is_even(n: Int): Bool {
    if n == 0 { ret true }
    ret is_odd(n - 1)
}
func is_odd(n: Int): Bool {
    if n == 0 { ret false }
    ret is_even(n - 1)
}
print(is_even(0))
print(is_even(7))
print(is_odd(7))
print(is_even(10))
