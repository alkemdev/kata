# Multiple allocations are independent
unsafe {
    let a = std.mem.alloc(4)
    let b = std.mem.alloc(4)
    std.mem.write(a, 0, "hello")
    std.mem.write(b, 0, "world")
    print(std.mem.read(a, 0))
    print(std.mem.read(b, 0))
    std.mem.free(a)
    std.mem.free(b)
}
