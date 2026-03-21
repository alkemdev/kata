# Buf[T]: pop returns Opt[T]
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(10)
buf.push(20)
print(buf.pop())
print(buf.pop())
print(buf.pop())
print(buf.len)
