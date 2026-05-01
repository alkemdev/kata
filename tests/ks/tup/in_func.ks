# Tuples as function arguments and return values
func swap(pair: Tup[Int, Int]): Tup[Int, Int] {
    ret (pair.1, pair.0)
}
let result = swap((3, 7))
print(result)
print(result.0)
print(result.1)
