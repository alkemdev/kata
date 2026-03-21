# Push on zero-capacity buf triggers grow
let buf = Buf[Int] { ptr: ptr_alloc(0), len: 0, cap: 0 }
buf.push(1)
buf.push(2)
buf.push(3)
print(buf.get(0))
print(buf.get(1))
print(buf.get(2))
print(buf.len)
