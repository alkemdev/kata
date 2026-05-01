# Variant containing tuple destructure: Val((a, b))
let v = Opt[Tup[Int, Int]].Val((3, 4))
let s = match v {
    Val((a, b)) -> a + b,
    Non() -> 0,
}
print(s)
