# A closure captures a binding whose value implements Drop. Observed:
# Drop fires *eagerly* when the defining scope (`make`) exits — every
# slot in the popped frame has its value dropped, regardless of whether
# the slot's Arc is still reachable through the returned closure. The
# closure body still resolves `r` through the captured slot, but the
# value's Drop side-effects have already run by the time `f()` is called.
# This locks in current behavior; revisit if Drop becomes ref-count-aware.
kind R { id: Int }

impl R as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

func make() {
    let r = R { id: 7 }
    func use_r() {
        print("inside: {r.id}")
    }
    ret use_r
}

print("before make")
let f = make()
print("after make")
f()
print("before reassign")
f = nil
print("after reassign")
