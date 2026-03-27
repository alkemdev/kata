# Interleaved push and pop
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(1)
a.push(2)
print(a.pop())
a.push(3)
print(a.get(0))
print(a.get(1))
print(a.len)
