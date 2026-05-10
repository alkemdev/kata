# Three-level variant nesting: Val(Val(Val(x))) stresses recursive descent.
let deep = Opt[Opt[Opt[Int]]].Val(Opt[Opt[Int]].Val(Opt[Int].Val(99)))
let mid_non = Opt[Opt[Opt[Int]]].Val(Opt[Opt[Int]].Val(Opt[Int].Non))
let inner_non = Opt[Opt[Opt[Int]]].Val(Opt[Opt[Int]].Non)
let outer_non = Opt[Opt[Opt[Int]]].Non

func describe(o: Opt[Opt[Opt[Int]]]): Str {
    ret match o {
        Val(Val(Val(x))) -> "deep {x}",
        Val(Val(Non())) -> "innermost none",
        Val(Non()) -> "middle none",
        Non() -> "outer none",
    }
}

print(describe(deep))
print(describe(mid_non))
print(describe(inner_non))
print(describe(outer_non))
