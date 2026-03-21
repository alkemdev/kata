enum Opt[T] {
    Val(T),
    Non,
}

let y = Opt[Int].Non
print(y)
