# Interleaved push and pop
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(1)
buf.push(2)
print(buf.pop())
buf.push(3)
print(buf.get(0))
print(buf.get(1))
print(buf.len)
