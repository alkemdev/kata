# Match on variant with multiple fields
enum Pair[A, B] { Both(A, B), Empty }

let p = Pair[Int, Str].Both(42, "hello")
match p {
    Both(n, s) -> print("{n}: {s}"),
    Empty -> print("empty"),
}
