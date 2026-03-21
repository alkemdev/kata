# std.mem — Memory management primitives
#
# Layer stack:
#   RawPtr       — prim Value, opaque handle to runtime storage
#   Ptr[T]       — kind, typed view over RawPtr
#   Buf[T]       — kind, allocated block (ptr + cap)
#
# Raw memory intrinsics (std.mem namespace, require unsafe):
#   std.mem.alloc(cap)              -> RawPtr
#   std.mem.free(raw: RawPtr)
#   std.mem.read(raw: RawPtr, idx)  -> Value
#   std.mem.write(raw: RawPtr, idx, val)
#   std.mem.capacity(raw: RawPtr)   -> Int
#   std.mem.len(raw: RawPtr)        -> Int

# ── Allocator interface ──────────────────────────────────────────

type Allocator {
    func make(self, cap: Int): RawPtr
    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr
    func free(self, raw: RawPtr)
}

kind HeapAllocator {}

impl HeapAllocator as Allocator {
    func make(self, cap: Int): RawPtr {
        unsafe { ret std.mem.alloc(cap) }
    }

    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr {
        unsafe {
            let new_raw = std.mem.alloc(new_cap)
            let i = 0
            while i < old_cap {
                std.mem.write(new_raw, i, std.mem.read(raw, i))
                i = i + 1
            }
            std.mem.free(raw)
            ret new_raw
        }
    }

    func free(self, raw: RawPtr) {
        unsafe { std.mem.free(raw) }
    }
}

let heap = HeapAllocator {}

# ── Ptr[T] — typed pointer ───────────────────────────────────────

kind Ptr[T] { raw: RawPtr }

impl Ptr[T] {
    func read(self, index: Int): T {
        unsafe { ret std.mem.read(self.raw, index) }
    }

    func write(self, index: Int, val: T) {
        unsafe { std.mem.write(self.raw, index, val) }
    }
}

# ── Buf[T] — allocated block ─────────────────────────────────────
#
# Buf is purely about memory lifecycle: allocate, grow, free.
# No len — that's Arr's concern. Buf just tracks ptr + capacity.

kind Buf[T] { ptr: Ptr[T], cap: Int }

impl Buf[T] {
    func read(self, index: Int): T {
        ret self.ptr.read(index)
    }

    func write(self, index: Int, val: T) {
        self.ptr.write(index, val)
    }

    func grow(self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 }
        let new_raw = heap.grow(self.ptr.raw, self.cap, new_cap)
        self.ptr = Ptr[T] { raw: new_raw }
        self.cap = new_cap
    }
}

impl Buf[T] as Drop {
    func drop(self) {
        heap.free(self.ptr.raw)
    }
}
