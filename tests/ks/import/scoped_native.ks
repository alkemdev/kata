# Scoped import preserves native intrinsics
import std.mem
let p = unsafe { std.mem.alloc(4) }
unsafe { std.mem.write(p, 0, 42) }
let val = unsafe { std.mem.read(p, 0) }
print(val)
unsafe { std.mem.free(p) }
