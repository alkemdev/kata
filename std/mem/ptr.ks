# mem.ptr — Ptr[T], typed pointer over RawPtr

kind Ptr[T] { raw: RawPtr }

impl Ptr[@T] {
    func read(self, index: Int): T {
        unsafe { ret mem.read(self.raw, index) }
    }

    func write(self, index: Int, val: T) {
        unsafe { mem.write(self.raw, index, val) }
    }
}
