# Ptr can store any value type
kind Point { x: Int, y: Int }

unsafe {
    let id = std.mem.alloc(5)
    std.mem.write(id, 0, 42)
    std.mem.write(id, 1, "hello")
    std.mem.write(id, 2, true)
    std.mem.write(id, 3, 3.14)
    std.mem.write(id, 4, Point { x: 1, y: 2 })
    print(std.mem.read(id, 0))
    print(std.mem.read(id, 1))
    print(std.mem.read(id, 2))
    print(std.mem.read(id, 3))
    print(std.mem.read(id, 4))
    std.mem.dealloc(id)
}
