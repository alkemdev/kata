# Wildcard sub-pattern inside a variant pattern
enum Pair[A, B] { Both(A, B), Empty, }

let p = Pair[Int, Str].Both(42, "hi")
let n = match p {
    Both(_, s) -> s,
    Empty() -> "empty",
}
print(n)
