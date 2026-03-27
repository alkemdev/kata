# Buf[T]: grow reallocates and preserves data
import mem.{Ptr, Buf, heap}

let buf = Buf[Str] { ptr: Ptr[Str] { raw: heap.make(2) }, cap: 2 }
buf.write(0, "a")
buf.write(1, "b")
buf.grow()
buf.write(2, "c")
print(buf.read(0))
print(buf.read(1))
print(buf.read(2))
print(buf.cap >= 4)
