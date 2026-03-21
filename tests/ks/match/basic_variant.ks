# Match on Opt variants with destructuring
let x = Opt[Int].Val(42)
let result = match x {
    Val(n) -> n * 2,
    Non -> 0,
}
print(result)

let y = Opt[Int].Non
let result2 = match y {
    Val(n) -> n,
    Non -> -1,
}
print(result2)
