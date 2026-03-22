# ? in binary operator should propagate
func might_fail(): Opt[Int] {
    ret Opt[Int].Non
}

func test(): Opt[Int] {
    let result = 1 + might_fail()?
    print("should not reach here")
    ret Opt[Int].Val(result)
}

print(test())
