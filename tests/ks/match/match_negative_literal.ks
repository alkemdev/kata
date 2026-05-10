# Single-case negative number literal pattern in match.
let x = -5
let s = match x {
    -5 -> "neg",
    _ -> "other",
}
print(s)

let y = 5
let t = match y {
    -5 -> "neg",
    _ -> "other",
}
print(t)
