# dsa.map — Map[K, V], hash map via open addressing
#
# Open addressing with linear probing. Slots are Empty, Del, or Used.
# Load factor threshold: resize at count > cap * 3 / 4.

import core.{Opt, Hash, GetItem, SetItem}
import mem.{Ptr, Buf, heap}
import dsa.arr.{Arr}

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
        let cap = 8
        let raw = mem.heap.make(cap)
        let i = 0
        while i < cap {
            unsafe { mem.write(raw, i, Slot[K, V].Empty) }
            i = i + 1
        }
        ret Map[K, V] {
            slots: Arr[Slot[K, V]] {
                buf: mem.Buf[Slot[K, V]] { ptr: mem.Ptr[Slot[K, V]] { raw: raw }, cap: cap },
                len: cap,
            },
            count: 0,
            cap: cap,
        }
    }

    func _idx(self, key: K): Int {
        ret key.hash().to_int() % self.cap
    }

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
        let old_slots = self.slots
        let old_cap = self.cap
        self.cap = self.cap * 2
        self.count = 0

        let raw = mem.heap.make(self.cap)
        let i = 0
        while i < self.cap {
            unsafe { mem.write(raw, i, Slot[K, V].Empty) }
            i = i + 1
        }
        self.slots = Arr[Slot[K, V]] {
            buf: mem.Buf[Slot[K, V]] { ptr: mem.Ptr[Slot[K, V]] { raw: raw }, cap: self.cap },
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
