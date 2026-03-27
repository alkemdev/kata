# Ptr[T] wraps RawPtr with typed read/write
import mem.{Ptr}
let raw = unsafe { mem.alloc(4) }
let p = Ptr[Int] { raw: raw }
p.write(0, 42)
p.write(1, 99)
print(p.read(0))
print(p.read(1))
unsafe { mem.free(raw) }
