# Calling a mem intrinsic outside any unsafe block is an error
let p = mem.alloc(4)
print(p)
