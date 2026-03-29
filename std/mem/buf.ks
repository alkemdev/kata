# mem.buf — Buf[T], allocated block with capacity

import core.lifecycle.{Drop}
import mem.ptr.{Ptr}
import mem.allocator.{heap}

kind Buf[T] { ptr: Ptr[T], cap: Int }

impl Buf[@T] {
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

impl Buf[@T] as Drop {
    func drop(self) {
        heap.free(self.ptr.raw)
    }
}
