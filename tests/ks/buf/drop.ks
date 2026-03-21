# Buf[T] Drop auto-frees via Ptr → RawPtr
func test() {
    let buf = Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }
    buf.write(0, 42)
    print(buf.read(0))
}

test()
print("done")
