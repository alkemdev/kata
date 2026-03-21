# Reading from a deallocated pointer is an error
unsafe {
    let id = std.mem.alloc(4)
    std.mem.write(id, 0, 42)
    std.mem.free(id)
    std.mem.read(id, 0)
}
