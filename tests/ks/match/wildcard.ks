# Wildcard catches unmatched arms
let x = Opt[Int].Non
let result = match x {
    Val(n) -> "val",
    _ -> "other",
}
print(result)
