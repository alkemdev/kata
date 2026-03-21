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
# Ptr wraps a runtime-managed allocation handle. The handle (_id) is
# an integer index into the interpreter's allocation table.
#
# All raw memory operations go through the std.mem namespace:
#   std.mem.alloc(cap)          allocate storage, return handle
#   std.mem.dealloc(id)         free storage (use-after-free errors)
#   std.mem.read(id, idx)       read element (no bounds check)
#   std.mem.write(id, idx, val) write element (pads with nil)
#   std.mem.grow(id, new_cap)   grow to at least new_cap
#   std.mem.capacity(id)        query allocated capacity
#   std.mem.len(id)             query number of written elements
#
# These intrinsics require an unsafe block. Ptr methods wrap them
# so that higher-level types (Buf, Arr) can use safe Ptr methods
# while keeping the unsafe boundary inside Ptr itself.

kind Ptr { _id: Int }

func ptr_alloc(cap: Int): Ptr {
    unsafe {
        ret Ptr { _id: std.mem.alloc(cap) }
    }
}

impl Ptr {
    func dealloc(self) {
        unsafe { std.mem.dealloc(self._id) }
    }

    func read(self, index: Int) {
        unsafe { ret std.mem.read(self._id, index) }
    }

    func write(self, index: Int, val) {
        unsafe { std.mem.write(self._id, index, val) }
    }

    func grow(self, new_cap: Int) {
        unsafe { std.mem.grow(self._id, new_cap) }
    }

    func capacity(self): Int {
        unsafe { ret std.mem.capacity(self._id) }
    }

    func len(self): Int {
        unsafe { ret std.mem.len(self._id) }
    }
}

impl Ptr as Drop {
    func drop(self) {
        unsafe { std.mem.dealloc(self._id) }
    }
}

# ── Buf[T] — typed growable buffer ────────────────────────────────
#
# Buf[T] wraps a Ptr with length and capacity tracking.
# Provides bounds-checked access and type-safe push/pop.
# Does NOT need its own Drop — the ptr field's Drop handles dealloc.

kind Buf[T] { ptr: Ptr, len: Int, cap: Int }

# Construction: Buf[Int] { ptr: ptr_alloc(0), len: 0, cap: 0 }
# Or use a helper that takes explicit capacity:
#   Buf[Int] { ptr: ptr_alloc(8), len: 0, cap: 8 }

impl Buf[T] {
    func get(self, index: Int): T {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        ret self.ptr.read(index)
    }

    func set(self, index: Int, val: T) {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        self.ptr.write(index, val)
    }

    func push(self, val: T) {
        if self.len == self.cap {
            self.grow()
        }
        self.ptr.write(self.len, val)
        self.len = self.len + 1
    }

    func pop(self): Opt[T] {
        if self.len == 0 {
            ret Opt[T].None
        }
        self.len = self.len - 1
        ret Opt[T].Some(self.ptr.read(self.len))
    }

    func grow(self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 }
        let old_id = self.ptr._id
        unsafe {
            let new_id = std.mem.alloc(new_cap)
            let i = 0
            while i < self.len {
                std.mem.write(new_id, i, std.mem.read(old_id, i))
                i = i + 1
            }
            std.mem.dealloc(old_id)
            self.ptr = Ptr { _id: new_id }
        }
        self.cap = new_cap
    }
}
