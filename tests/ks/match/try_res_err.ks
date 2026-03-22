# ? on Res.Err early-returns the error
func safe_div(a: Int, b: Int): Res[Int, Str] {
    if b == 0 {
        ret Res[Int, Str].Err("division by zero")
    }
    ret Res[Int, Str].Val(a / b)
}

func double_div(a: Int, b: Int): Res[Int, Str] {
    let n = safe_div(a, b)?
    ret Res[Int, Str].Val(n * 2)
}

print(double_div(10, 0))
