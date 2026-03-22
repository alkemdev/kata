# ? in assignment should propagate, not be swallowed
func might_fail(): Opt[Int] {
    ret Opt[Int].Non
}

func test(): Opt[Int] {
    let x = 0
    x = might_fail()?
    print("should not reach here")
    ret Opt[Int].Val(x)
}

print(test())
