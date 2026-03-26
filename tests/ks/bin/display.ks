# Bin displays as <bin:N bytes>
import std.dsa
import std.mem

let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
arr.push(Byte(0xab))
arr.push(Byte(0xcd))

let b = arr.to_bin()
print(b)
