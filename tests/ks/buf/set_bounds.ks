# set at invalid index errors
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(1)
buf.set(5, 99)
