# Buf[T]: out-of-bounds get errors
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push(1)
buf.get(5)
