# Arr[T]: pop returns Opt[T]
let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(10)
a.push(20)
print(a.pop())
print(a.pop())
print(a.pop())
print(a.len)
