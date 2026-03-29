# mem.allocator — Allocator interface, HeapAllocator, heap singleton

type Allocator {
    func make(self, cap: Int): RawPtr
    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr
    func free(self, raw: RawPtr)
}

kind HeapAllocator {}

impl HeapAllocator as Allocator {
    func make(self, cap: Int): RawPtr {
        unsafe { ret mem.alloc(cap) }
    }

    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr {
        unsafe {
            let new_raw = mem.alloc(new_cap)
            let i = 0
            while i < old_cap {
                mem.write(new_raw, i, mem.read(raw, i))
                i = i + 1
            }
            mem.free(raw)
            ret new_raw
        }
    }

    func free(self, raw: RawPtr) {
        unsafe { mem.free(raw) }
    }
}

let heap = HeapAllocator {}
