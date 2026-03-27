# Nested for loops over arrays
import mem.{Ptr, Buf, heap}

let a = Arr[Str] { buf: Buf[Str] { ptr: Ptr[Str] { raw: heap.make(4) }, cap: 4 }, len: 0 }
a.push("a")
a.push("b")

let b = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
b.push(1)
b.push(2)

for s in a {
    for n in b {
        print("{s}{n}")
    }
}
