# Closure body has unsafe block; captured environment doesn't change unsafe gating
let base = 1000
func f(): Int {
    unsafe {
        let p = mem.alloc(1)
        mem.write(p, 0, base)
        let v = mem.read(p, 0)
        mem.free(p)
        ret v
    }
}
print(f())
print(f())
