# Buf[T]: Ptr field auto-drops when Buf goes out of scope
func test() {
    let buf = Buf[Int] { ptr: ptr_alloc(4), len: 0, cap: 4 }
    buf.push(42)
    print(buf.get(0))
    # buf goes out of scope — Ptr field Drop fires
}

test()
print("done")
