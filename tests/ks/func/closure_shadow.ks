# A `let` inside a closure body creates a fresh slot in the closure's
# call frame, NOT a write through the captured slot. The outer name's
# value should be unchanged after the closure runs.
let x = 1
func make() {
    func inner() {
        let x = 99
        print(x)
    }
    ret inner
}
let f = make()
f()
print(x)
