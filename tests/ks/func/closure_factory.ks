# A factory returns a closure that owns its own state. Each call to the
# factory creates a fresh slot for `count`, so two counters built from
# the same factory don't share state with each other — but each one's
# returned closure shares state with the factory's `count` binding.
func make_counter() {
    let count = 0
    func incr(): Int {
        count = count + 1
        ret count
    }
    ret incr
}

let c1 = make_counter()
let c2 = make_counter()
print(c1())
print(c1())
print(c1())
print(c2())
print(c1())
print(c2())
