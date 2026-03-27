# Array is still usable after iteration (snapshot semantics)
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push(10)
a.push(20)

for x in a {
    print(x)
}

# Array still works after iteration
print(a.get(0))
print(a.len)
a.push(30)
print(a.len)
