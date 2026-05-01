# Negative number literal patterns
let xs = [-2, -1, 0, 1, 2]
for x in xs {
    let s = match x {
        -2 -> "neg-two",
        -1 -> "neg-one",
        0 -> "zero",
        1 -> "one",
        _ -> "other",
    }
    print(s)
}
