# Push on zero-capacity Arr triggers grow
let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(0) }, cap: 0 }, len: 0 }
a.push(1)
a.push(2)
a.push(3)
print(a.get(0))
print(a.get(1))
print(a.get(2))
print(a.len)
