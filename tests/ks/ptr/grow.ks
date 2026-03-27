# HeapAllocator.grow: allocate, copy, free old, return new
import mem.{heap}

let raw = heap.make(2)
unsafe {
    mem.write(raw, 0, "a")
    mem.write(raw, 1, "b")
}
let new_raw = heap.grow(raw, 2, 10)
unsafe {
    mem.write(new_raw, 5, "f")
    print(mem.read(new_raw, 0))
    print(mem.read(new_raw, 5))
}
heap.free(new_raw)
