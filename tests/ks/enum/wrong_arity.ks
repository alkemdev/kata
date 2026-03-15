enum Opt[T] {
    Some(T),
    None,
}

Opt[Int].Some(1, 2)
