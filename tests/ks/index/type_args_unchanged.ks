# Type instantiation still works with []
let x = Opt[Int].Val(42)
print(x.unwrap())
