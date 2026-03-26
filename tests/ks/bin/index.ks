# Bin supports indexing via get_item, returns Byte
import std.dsa
import std.mem

let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
arr.push(Byte(0x01))
arr.push(Byte(0x02))
arr.push(Byte(0x03))

let b = arr.to_bin()
print(b[0])
print(b[1])
print(b[2])
print(typeof(b[0]))
