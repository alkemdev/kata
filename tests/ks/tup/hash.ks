# Tuples are hashable
let a = (1, "hello")
let b = (1, "hello")
print(typeof(a.hash()))
print(a.hash() == b.hash())
