# Multiple allocations are independent
unsafe {
    let a = mem.alloc(4)
    let b = mem.alloc(4)
    mem.write(a, 0, "hello")
    mem.write(b, 0, "world")
    print(mem.read(a, 0))
    print(mem.read(b, 0))
    mem.free(a)
    mem.free(b)
}
