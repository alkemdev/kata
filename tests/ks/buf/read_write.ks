# Buf[T]: raw read and write (no bounds checking)
import mem.{Ptr, Buf, heap}

let buf = Buf[Int] { ptr: Ptr[Int] { raw: heap.make(4) }, cap: 4 }
buf.write(0, 10)
buf.write(1, 20)
buf.write(2, 30)
print(buf.read(0))
print(buf.read(1))
print(buf.read(2))
