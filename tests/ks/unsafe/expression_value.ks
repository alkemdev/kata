# unsafe { ... } is an expression yielding the last value
let p = unsafe { mem.alloc(3) }
unsafe { mem.write(p, 0, 100) }
let v = unsafe { mem.read(p, 0) }
print(v)
unsafe { mem.free(p) }
print("ok")
