# Scoped import preserves native intrinsics
import mem
let p = unsafe { mem.alloc(4) }
unsafe { mem.write(p, 0, 42) }
let val = unsafe { mem.read(p, 0) }
print(val)
unsafe { mem.free(p) }
