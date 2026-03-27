# Arr[Byte].to_bin() creates an interned Bin value
import mem.{Ptr, Buf, heap}

let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
arr.push(Byte(0xff))
arr.push(Byte(0x00))
arr.push(Byte(0xab))

let b = arr.to_bin()
print(typeof(b))
print(b.len())
