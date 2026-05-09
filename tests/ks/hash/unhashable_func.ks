# Hashing a tuple that contains a function value is a structured error,
# not a panic. (The tuple itself looks hashable, but recursive
# is_hashable catches the unhashable function field.)
func id(x: Int): Int { ret x }
let pair = (id, 5)
print(pair.hash())
