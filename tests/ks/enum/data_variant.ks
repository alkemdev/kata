enum Opt[T] {
    Val(T),
    Non,
}

let x = Opt[Int].Val(42)
print(x)
