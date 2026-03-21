# Arr auto-frees when scope exits (Buf Drop → Ptr → RawPtr)
func test() {
    let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }, len: 0 }
    a.push(42)
    a.push(99)
    print(a.get(0))
    print(a.get(1))
}

test()
print("done")
