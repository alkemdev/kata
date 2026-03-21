# Grow an allocation and write to extended region
unsafe {
    let id = std.mem.alloc(2)
    std.mem.write(id, 0, "a")
    std.mem.write(id, 1, "b")
    std.mem.grow(id, 10)
    std.mem.write(id, 5, "f")
    print(std.mem.read(id, 0))
    print(std.mem.read(id, 5))
    let cap = std.mem.capacity(id)
    print(cap >= 10)
    std.mem.dealloc(id)
}
