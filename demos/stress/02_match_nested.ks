# Stress 02: Match on deeply nested patterns
# Combines variant + tuple + literal + wildcard at multiple depths.
# Tests whether the pattern matcher handles full nesting correctly.

enum Tag {
    A,
    B,
    C(Str),
}

# Opt[Tup[Tag, Tup[Int, Int]]]
func describe(v: Opt[Tup[Tag, Tup[Int, Int]]]): Str {
    ret match v {
        Val((A(), (0, 0)))      -> "A at origin",
        Val((A(), (x, y)))      -> "A at ({x}, {y})",
        Val((B(), (1, 1)))      -> "B at unit",
        Val((B(), (_, y)))      -> "B at y={y}",
        Val((C(name), (x, y))) -> "C[{name}] at ({x}, {y})",
        Non()                    -> "missing",
    }
}

let cases = [
    Opt[Tup[Tag, Tup[Int, Int]]].Val((Tag.A, (0, 0))),
    Opt[Tup[Tag, Tup[Int, Int]]].Val((Tag.A, (3, 4))),
    Opt[Tup[Tag, Tup[Int, Int]]].Val((Tag.B, (1, 1))),
    Opt[Tup[Tag, Tup[Int, Int]]].Val((Tag.B, (5, 7))),
    Opt[Tup[Tag, Tup[Int, Int]]].Val((Tag.C("hello"), (-1, -2))),
    Opt[Tup[Tag, Tup[Int, Int]]].Non,
]

for c in cases {
    print(describe(c))
}
