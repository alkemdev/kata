# Bin index out of bounds is a runtime error
import mem.{Ptr, Buf, heap}

let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
arr.push(Byte(0x01))

let b = arr.to_bin()
b[5]
