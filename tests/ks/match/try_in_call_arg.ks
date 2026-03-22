# ? in function call argument should propagate
func might_fail(): Opt[Int] {
    ret Opt[Int].Non
}

func use_val(x: Int) {
    print(x)
}

func test(): Opt[Int] {
    use_val(might_fail()?)
    print("should not reach here")
    ret Opt[Int].Val(0)
}

print(test())
