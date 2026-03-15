enum Res[T, E] {
    Ok(T),
    Err(E),
}

let ok = Res[Int, Str].Ok(42)
let err = Res[Int, Str].Err("bad")
print(ok)
print(err)
