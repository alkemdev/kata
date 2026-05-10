# Two-level variant nesting: Val(Val(x)) — both layers refutable
let inner_val = Opt[Int].Val(42)
let outer_val = Opt[Opt[Int]].Val(inner_val)
let outer_inner_non = Opt[Opt[Int]].Val(Opt[Int].Non)
let outer_non = Opt[Opt[Int]].Non

func describe(o: Opt[Opt[Int]]): Str {
    ret match o {
        Val(Val(x)) -> "got {x}",
        Val(Non()) -> "inner none",
        Non() -> "outer none",
    }
}

print(describe(outer_val))
print(describe(outer_inner_non))
print(describe(outer_non))
