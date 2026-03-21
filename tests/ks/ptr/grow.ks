# HeapAllocator.grow: allocate, copy, free old, return new
let raw = heap.make(2)
unsafe {
    std.mem.write(raw, 0, "a")
    std.mem.write(raw, 1, "b")
}
let new_raw = heap.grow(raw, 2, 10)
unsafe {
    std.mem.write(new_raw, 5, "f")
    print(std.mem.read(new_raw, 0))
    print(std.mem.read(new_raw, 5))
}
heap.free(new_raw)
