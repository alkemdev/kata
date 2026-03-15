enum Opt[T] {
    Some(T),
    None,
}

let x = Opt[Int].Some(42)
print(x)
