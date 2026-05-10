# Nested unsafe blocks: inner unsafe is fine, mem ops work inside
unsafe {
    unsafe {
        let p = mem.alloc(2)
        mem.write(p, 0, 7)
        mem.write(p, 1, 9)
        print(mem.read(p, 0))
        print(mem.read(p, 1))
        mem.free(p)
    }
    print("outer-still-unsafe")
}
print("done")
