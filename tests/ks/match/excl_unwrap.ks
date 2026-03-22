# ! operator unwraps Val
let x = Opt[Int].Val(42)!
print(x)

let y = Res[Int, Str].Val(99)!
print(y)
