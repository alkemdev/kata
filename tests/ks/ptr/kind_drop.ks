# Ptr Drop auto-deallocates when scope exits
func test() {
    let p = ptr_alloc(4)
    p.write(0, 42)
    print(p.read(0))
    # p goes out of scope here — Drop calls std.mem.dealloc
}

test()
print("done")
