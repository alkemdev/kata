# Chained ? operators
func add_elements(arr: Arr[Int], i: Int, j: Int): Opt[Int] {
    let a = arr.get(i)?
    let b = arr.get(j)?
    ret Opt[Int].Val(a + b)
}

let nums = [10, 20, 30]
print(add_elements(nums, 0, 2))
print(add_elements(nums, 0, 99))
