# Function body contains unsafe block; unsafe scope is contained by the call
func roundtrip(n: Int): Int {
    unsafe {
        let p = mem.alloc(1)
        mem.write(p, 0, n)
        let v = mem.read(p, 0)
        mem.free(p)
        ret v
    }
}
print(roundtrip(42))
print(roundtrip(7))
