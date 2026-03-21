# Buf[T]: push elements and get them back
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(10)
buf.push(20)
buf.push(30)
print(buf.get(0))
print(buf.get(1))
print(buf.get(2))
print(buf.len)
