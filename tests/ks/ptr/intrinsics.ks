# Raw ptr intrinsics: alloc, write, read, len, capacity, dealloc
unsafe {
    let id = std.mem.alloc(4)
    std.mem.write(id, 0, 10)
    std.mem.write(id, 1, 20)
    std.mem.write(id, 2, 30)
    print(std.mem.read(id, 0))
    print(std.mem.read(id, 1))
    print(std.mem.read(id, 2))
    print(std.mem.len(id))
    let cap = std.mem.capacity(id)
    print(cap >= 4)
    std.mem.dealloc(id)
}
print("done")
