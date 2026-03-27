# Push onto an array created from a literal
import mem.{Ptr, Buf, heap}

let a = [10, 20]
a.push(30)
a.push(40)
print(a.get(2))
print(a.get(3))
print(a.len)
