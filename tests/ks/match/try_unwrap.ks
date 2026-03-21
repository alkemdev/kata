# ? operator unwraps Val, early-returns Non
func double_get(arr: Arr[Int], i: Int): Opt[Int] {
    let val = arr.get(i)?
    ret Opt[Int].Val(val * 2)
}

let a = [10, 20, 30]
print(double_get(a, 0))
print(double_get(a, 1))
print(double_get(a, 99))
