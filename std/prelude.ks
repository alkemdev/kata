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
