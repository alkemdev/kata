# Buf works with non-Int types
let buf = Buf[Str] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push("hello")
buf.push("world")
print(buf.get(0))
print(buf.get(1))
print(buf.pop())
