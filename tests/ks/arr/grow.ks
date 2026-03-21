# Arr[T]: auto-grows past initial capacity
let a = Arr[Str] { buf: Buf[Str] { ptr: Ptr[Str] { raw: heap.make(2) }, cap: 2 }, len: 0 }
a.push("a")
a.push("b")
a.push("c")
a.push("d")
a.push("e")
print(a.get(0))
print(a.get(4))
print(a.len)
