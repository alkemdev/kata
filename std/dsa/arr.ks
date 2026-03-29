# dsa.arr — Arr[T], safe iterable growable array
#
# Arr adds the "valid region" (len) on top of Buf's raw capacity.
# Bounds-checked access, push/pop, type-safe — all at this layer.
#
# Static methods use mem.X paths (run in caller scope where only the
# scoped `mem` module is available, not selective imports).

import core.{Opt, GetItem, SetItem, ToBin}
import mem.{Ptr, Buf, heap}

kind Arr[T] { buf: Buf[T], len: Int }

impl Arr[@T] {
    func new(): Arr[T] {
        let cap = 4
        let raw = mem.heap.make(cap)
        ret Arr[T] {
            buf: mem.Buf[T] { ptr: mem.Ptr[T] { raw: raw }, cap: cap },
            len: 0,
        }
    }

    func push(self, val: T) {
        if self.len == self.buf.cap {
            let new_cap = if self.buf.cap == 0 { 4 } else { self.buf.cap * 2 }
            let new_raw = heap.grow(self.buf.ptr.raw, self.buf.cap, new_cap)
            self.buf = Buf[T] { ptr: Ptr[T] { raw: new_raw }, cap: new_cap }
        }
        self.buf.ptr.write(self.len, val)
        self.len = self.len + 1
    }

    func pop(self): Opt[T] {
        if self.len == 0 {
            ret Opt[T].Non
        }
        self.len = self.len - 1
        ret Opt[T].Val(self.buf.read(self.len))
    }

    func get(self, index: Int): Opt[T] {
        if index < 0 || index >= self.len {
            ret Opt[T].Non
        }
        ret Opt[T].Val(self.buf.read(index))
    }

    func set(self, index: Int, val: T): Bool {
        if index < 0 || index >= self.len {
            ret false
        }
        self.buf.write(index, val)
        ret true
    }
}

# ── ArrIter[T] ───────────────────────────────────────────────────

kind ArrIter[T] { arr: Arr[T], idx: Int }

impl ArrIter[@T] as Iter[T] {
    func next(self): Opt[T] {
        if self.idx >= self.arr.len {
            ret Opt[T].Non
        }
        let val = self.arr.buf.read(self.idx)
        self.idx = self.idx + 1
        ret Opt[T].Val(val)
    }
}

impl Arr[@T] as GetItem[Int, T] {
    func get_item(self, key: Int): T {
        if key < 0 || key >= self.len {
            panic("index out of bounds: {key}, len {self.len}")
        }
        ret self.buf.read(key)
    }
}

impl Arr[@T] as SetItem[Int, T] {
    func set_item(self, key: Int, val: T) {
        if key < 0 || key >= self.len {
            panic("index out of bounds: {key}, len {self.len}")
        }
        self.buf.write(key, val)
    }
}

impl Arr[@T] as ToIter[T] {
    func to_iter(self): ArrIter[T] {
        ret ArrIter[T] { arr: self, idx: 0 }
    }
}

impl Arr[Byte] as ToBin {
    func to_bin(self): Bin {
        unsafe {
            ret mem.bin_from_raw(self.buf.ptr.raw, self.len)
        }
    }
}
