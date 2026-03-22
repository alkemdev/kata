# Chained ? on Res across function calls
func safe_div(a: Int, b: Int): Res[Int, Str] {
    if b == 0 {
        ret Res[Int, Str].Err("division by zero")
    }
    ret Res[Int, Str].Val(a / b)
}

func compute(a: Int, b: Int, c: Int): Res[Int, Str] {
    let x = safe_div(a, b)?
    let y = safe_div(x, c)?
    ret Res[Int, Str].Val(y)
}

print(compute(100, 5, 2))
print(compute(100, 0, 2))
print(compute(100, 5, 0))
