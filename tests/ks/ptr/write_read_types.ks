# Ptr can store any value type
kind Point { x: Int, y: Int }

unsafe {
    let id = mem.alloc(5)
    mem.write(id, 0, 42)
    mem.write(id, 1, "hello")
    mem.write(id, 2, true)
    mem.write(id, 3, 3.14)
    mem.write(id, 4, Point { x: 1, y: 2 })
    print(mem.read(id, 0))
    print(mem.read(id, 1))
    print(mem.read(id, 2))
    print(mem.read(id, 3))
    print(mem.read(id, 4))
    mem.free(id)
}
