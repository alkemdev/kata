enum Res[T, E] {
    Val(T),
    Err(E),
}

let ok = Res[Int, Str].Val(42)
let err = Res[Int, Str].Err("bad")
print(ok)
print(err)
