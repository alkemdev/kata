# Arr[Int]: set with wrong type errors
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(1)
a.set(0, "wrong")
