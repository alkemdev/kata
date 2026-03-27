# Raw ptr intrinsics: alloc, write, read, len, capacity, dealloc
unsafe {
    let id = mem.alloc(4)
    mem.write(id, 0, 10)
    mem.write(id, 1, 20)
    mem.write(id, 2, 30)
    print(mem.read(id, 0))
    print(mem.read(id, 1))
    print(mem.read(id, 2))
    print(mem.len(id))
    let cap = mem.capacity(id)
    print(cap >= 4)
    mem.free(id)
}
print("done")
