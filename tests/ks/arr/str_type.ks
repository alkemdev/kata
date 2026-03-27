# Arr works with non-Int element types
import mem.{Ptr, Buf, heap}

let a = Arr[Str] { buf: Buf[Str] { ptr: Ptr[Str] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push("hello")
a.push("world")
print(a.get(0))
print(a.get(1))
print(a.pop())
