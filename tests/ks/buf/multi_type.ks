# Buf works with different element types
let ints = Buf[Int] { ptr: Ptr[Int] { raw: heap.make(2) }, cap: 2 }
let strs = Buf[Str] { ptr: Ptr[Str] { raw: heap.make(2) }, cap: 2 }
ints.write(0, 42)
strs.write(0, "hello")
print(ints.read(0))
print(strs.read(0))
