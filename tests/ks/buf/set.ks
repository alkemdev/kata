# Buf[T]: set overwrites an element
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(10)
buf.push(20)
buf.push(30)
buf.set(1, 99)
print(buf.get(0))
print(buf.get(1))
print(buf.get(2))
