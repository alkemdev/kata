enum Opt[T] {
    Val(T),
    Non,
}

Opt[Int].Val(1, 2)
