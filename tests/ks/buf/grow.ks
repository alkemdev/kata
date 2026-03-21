# Buf[T]: grows automatically when capacity exceeded
let buf = Buf[Str] { ptr: ptr_alloc(2), len: 0, cap: 2 }
buf.push("a")
buf.push("b")
buf.push("c")
buf.push("d")
buf.push("e")
print(buf.get(0))
print(buf.get(4))
print(buf.len)
print(buf.cap >= 5)
