# Reading from a deallocated pointer is an error
unsafe {
    let id = mem.alloc(4)
    mem.write(id, 0, 42)
    mem.free(id)
    mem.read(id, 0)
}
