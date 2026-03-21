# Array literal as function return value
func make_nums(): Arr[Int] {
    ret [10, 20, 30]
}

let a = make_nums()
for x in a {
    print(x)
}
