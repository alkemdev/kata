# Buf[Int]: pushing a Str is a type error
let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
buf.push("wrong")
