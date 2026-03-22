# cont skips elements
let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(8) }, cap: 8 }, len: 0 }
a.push(1)
a.push(2)
a.push(3)
a.push(4)
a.push(5)

for x in a {
    if x == 2 || x == 4 {
        cont
    }
    print(x)
}
