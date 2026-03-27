# Pop on empty Arr returns None
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(0) }, cap: 0 }, len: 0 }
print(a.pop())
print(a.pop())
