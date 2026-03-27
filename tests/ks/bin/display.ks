# Bin displays as b'...' literal — round-trippable format
import mem.{Ptr, Buf, heap}

let arr = Arr[Byte] { buf: Buf[Byte] { ptr: Ptr[Byte] { raw: heap.make(4) }, cap: 4 }, len: 0 }
arr.push(Byte(0xab))
arr.push(Byte(0xcd))

let b = arr.to_bin()
print(b)

# Printable ASCII shows as chars
let hello = b'hello'
print(hello)

# Mixed printable + non-printable
let mixed = b'hi\xff\n'
print(mixed)
