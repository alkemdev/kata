# Identical Bin values are interned — equality is pointer-fast
import std.dsa
import std.mem

func make_bin(): Bin {
    let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
    arr.push(Byte(0x01))
    arr.push(Byte(0x02))
    ret arr.to_bin()
}

let a = make_bin()
let b = make_bin()
print(a == b)
print(a != b)
