# KataScript standard prelude
# Auto-loaded before user code.

enum Opt[T] {
    Some(T),
    None,
}

enum Res[T, E] {
    Ok(T),
    Err(E),
}

# Iteration protocol interfaces
type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}

# Lifecycle protocols
type Drop {
    func drop(self)
}

type Copy {
    func copy(self): Self
}

type Dupe {
    func dupe(self): Self
}

# ── Memory management ──────────────────────────────────────────────
#
# Layer stack:
#   RawPtr       — prim Value, opaque handle to runtime storage
#   Ptr[T]       — kind, typed view over RawPtr
#   Buf[T]       — kind, allocated block (ptr + cap)
#   Arr[T]       — kind, safe array (buf + len + iteration)
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

# ── Arr[T] — safe, iterable array ────────────────────────────────
#
# Arr adds the "valid region" (len) on top of Buf's raw capacity.
# Bounds-checked access, push/pop, iteration — all at this layer.

kind Arr[T] { buf: Buf[T], len: Int }

impl Arr[T] {
    func push(self, val: T) {
        if self.len == self.buf.cap {
            # Inline grow — can't use self.buf.grow() because nested
            # method copy-out doesn't write back to self.buf.
            let new_cap = if self.buf.cap == 0 { 4 } else { self.buf.cap * 2 }
            let new_raw = heap.grow(self.buf.ptr.raw, self.buf.cap, new_cap)
            self.buf = Buf[T] { ptr: Ptr[T] { raw: new_raw }, cap: new_cap }
        }
        self.buf.ptr.write(self.len, val)
        self.len = self.len + 1
    }

    func pop(self): Opt[T] {
        if self.len == 0 {
            ret Opt[T].None
        }
        self.len = self.len - 1
        ret Opt[T].Some(self.buf.read(self.len))
    }

    func get(self, index: Int): T {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        ret self.buf.read(index)
    }

    func set(self, index: Int, val: T) {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        self.buf.write(index, val)
    }
}
