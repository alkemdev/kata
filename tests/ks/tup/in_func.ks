# Tuples as function arguments and return values
func swap(pair: Tup[Int, Int]): Tup[Int, Int] {
    ret (pair._1, pair._0)
}
let result = swap((3, 7))
print(result)
print(result._0)
print(result._1)
