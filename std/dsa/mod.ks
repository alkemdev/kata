# std.dsa — Data structures and algorithms
#
# Arr[T] — safe, iterable, growable array

import std.mem

# ── Arr[T] — safe, iterable array ────────────────────────────────
#
# Arr adds the "valid region" (len) on top of Buf's raw capacity.
# Bounds-checked access, push/pop, type-safe — all at this layer.

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
