# dsa — Data structures and algorithms
#
# Arr[T] — safe, iterable, growable array

import core.{Opt, Hash, GetItem, SetItem, ToBin}
import mem.{Ptr, Buf, heap}

# ── Arr[T] — safe, iterable array ────────────────────────────────
#
# Arr adds the "valid region" (len) on top of Buf's raw capacity.
# Bounds-checked access, push/pop, type-safe — all at this layer.

kind Arr[T] { buf: Buf[T], len: Int }

impl Arr[@T] {
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

# ── ArrIter[T] — array iterator ──────────────────────────────────

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

# ── Bin conversion ──────────────────────────────────────────────

impl Arr[Byte] as ToBin {
    func to_bin(self): Bin {
        unsafe {
            ret mem.bin_from_raw(self.buf.ptr.raw, self.len)
        }
    }
}

# ── Map[K, V] — hash map via open addressing ────────────────────
#
# Open addressing with linear probing. Slots are Empty, Del, or Used.
# Load factor threshold: resize at count > cap * 3 / 4.
# Uses Arr.set() for writes to avoid nested index assignment.

enum Slot[K, V] {
    Empty,
    Del,
    Used(K, V),
}

kind Map[K, V] {
    slots: Arr[Slot[K, V]],
    count: Int,
    cap: Int,
}

impl Map[@K, @V] {
    func new(): Map[K, V] {
        import mem.{Ptr, Buf, heap}
        let cap = 8
        let raw = heap.make(cap)
        let i = 0
        while i < cap {
            unsafe { mem.write(raw, i, Slot[K, V].Empty) }
            i = i + 1
        }
        ret Map[K, V] {
            slots: Arr[Slot[K, V]] {
                buf: Buf[Slot[K, V]] { ptr: Ptr[Slot[K, V]] { raw: raw }, cap: cap },
                len: cap,
            },
            count: 0,
            cap: cap,
        }
    }

    func _idx(self, key: K): Int {
        ret key.hash().to_int() % self.cap
    }

    # Returns (index, found) as a tuple.
    func _find(self, key: K): Tup[Int, Bool] {
        let start = self._idx(key)
        let i = start
        while true {
            let slot = self.slots[i]
            match slot {
                Empty -> ret (i, false),
                Del -> {},
                Used(k, v) -> {
                    if k == key {
                        ret (i, true)
                    }
                },
            }
            i = (i + 1) % self.cap
            if i == start {
                ret (i, false)
            }
        }
        ret (0, false)
    }

    func _grow(self) {
        import mem.{Ptr, Buf, heap}
        let old_slots = self.slots
        let old_cap = self.cap
        self.cap = self.cap * 2
        self.count = 0

        let raw = heap.make(self.cap)
        let i = 0
        while i < self.cap {
            unsafe { mem.write(raw, i, Slot[K, V].Empty) }
            i = i + 1
        }
        self.slots = Arr[Slot[K, V]] {
            buf: Buf[Slot[K, V]] { ptr: Ptr[Slot[K, V]] { raw: raw }, cap: self.cap },
            len: self.cap,
        }

        let j = 0
        while j < old_cap {
            let slot = old_slots[j]
            match slot {
                Used(k, v) -> self.set(k, v),
                _ -> {},
            }
            j = j + 1
        }
    }

    func get(self, key: K): Opt[V] {
        let result = self._find(key)
        if result._1 {
            let slot = self.slots[result._0]
            match slot {
                Used(k, v) -> ret Opt[V].Val(v),
                _ -> {},
            }
        }
        ret Opt[V].Non
    }

    func set(self, key: K, val: V) {
        if (self.count + 1) * 4 > self.cap * 3 {
            self._grow()
        }
        let result = self._find(key)
        let idx = result._0
        let found = result._1
        self.slots.set(idx, Slot[K, V].Used(key, val))
        if !found {
            self.count = self.count + 1
        }
    }

    func del(self, key: K): Bool {
        let result = self._find(key)
        if result._1 {
            self.slots.set(result._0, Slot[K, V].Del)
            self.count = self.count - 1
            ret true
        }
        ret false
    }

    func has(self, key: K): Bool {
        ret self._find(key)._1
    }

    func len(self): Int {
        ret self.count
    }
}

impl Map[@K, @V] as GetItem[K, V] {
    func get_item(self, key: K): V {
        let result = self._find(key)
        if result._1 {
            let slot = self.slots[result._0]
            match slot {
                Used(k, v) -> ret v,
                _ -> {},
            }
        }
        panic("key not found in map")
    }
}

impl Map[@K, @V] as SetItem[K, V] {
    func set_item(self, key: K, val: V) {
        self.set(key, val)
    }
}

kind MapIter[K, V] {
    slots: Arr[Slot[K, V]],
    cap: Int,
    idx: Int,
}

impl MapIter[@K, @V] as Iter[Tup[K, V]] {
    func next(self): Opt[Tup[K, V]] {
        while self.idx < self.cap {
            let slot = self.slots[self.idx]
            self.idx = self.idx + 1
            match slot {
                Used(k, v) -> ret Opt[Tup[K, V]].Val((k, v)),
                _ -> cont,
            }
        }
        ret Opt[Tup[K, V]].Non
    }
}

impl Map[@K, @V] as ToIter[Tup[K, V]] {
    func to_iter(self): MapIter[K, V] {
        ret MapIter[K, V] { slots: self.slots, cap: self.cap, idx: 0 }
    }
}
