# Tuple containing variant patterns: (Val(x), Non())
let a = Opt[Int].Val(7)
let b = Opt[Int].Non
let pair_val_non = (a, b)
let pair_val_val = (Opt[Int].Val(1), Opt[Int].Val(2))
let pair_non_non = (Opt[Int].Non, Opt[Int].Non)

func describe(p: Tup[Opt[Int], Opt[Int]]): Str {
    ret match p {
        (Val(x), Non()) -> "left {x}, right none",
        (Val(x), Val(y)) -> "both {x} {y}",
        (Non(), _) -> "left none",
    }
}

print(describe(pair_val_non))
print(describe(pair_val_val))
print(describe(pair_non_non))
