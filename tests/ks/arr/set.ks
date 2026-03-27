# Arr[T]: set overwrites an element
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(10)
a.push(20)
a.push(30)
a.set(1, 99)
print(a.get(0))
print(a.get(1))
print(a.get(2))
