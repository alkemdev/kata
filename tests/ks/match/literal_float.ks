# Float literal patterns (positive and negative)
let xs = [1.5, -2.5, 0.0, 99.9]
for x in xs {
    let s = match x {
        1.5 -> "one-half",
        -2.5 -> "neg-two-half",
        0.0 -> "zero",
        _ -> "other",
    }
    print(s)
}
