# Stress 03: Closures capturing mutable state
# Probes the closure semantics for state mutation.
# Compare:
#   (a) outer-scope mutation via top-level closure
#   (b) factory-style: function returning a counter
#   (c) state via a mutable struct (the workaround)

print("--- (a) top-level outer-mutating closure ---")
let count = 0
func incr_global() {
    count = count + 1
    print("inside incr_global: count={count}")
}
incr_global()
incr_global()
incr_global()
print("after calls, top-level count={count}")

print("--- (b) factory-style closure returning a func ---")
func make_counter() {
    let n = 0
    func step(): Int {
        n = n + 1
        ret n
    }
    ret step
}
let c1 = make_counter()
print(c1())
print(c1())
print(c1())
let c2 = make_counter()
print(c2())   # expected: 1 — independent of c1?
print(c1())   # expected: 4 — continues from c1's last value?

print("--- (c) struct as mutable state holder ---")
kind Counter { n: Int }
impl Counter {
    func step(self): Int {
        self.n = self.n + 1
        ret self.n
    }
}
let c3 = Counter { n: 0 }
print(c3.step())
print(c3.step())
print(c3.step())
