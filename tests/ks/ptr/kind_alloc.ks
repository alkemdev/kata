# Use Ptr kind to allocate, write, and read
let p = ptr_alloc(4)
p.write(0, "hello")
p.write(1, "world")
print(p.read(0))
print(p.read(1))
print(p.capacity() >= 4)
p.dealloc()
