# Arr[T]: set at invalid index errors
let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(1)
a.set(5, 99)
