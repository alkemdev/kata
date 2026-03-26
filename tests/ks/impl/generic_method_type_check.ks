# Generic method type params are enforced at call time
kind Box[T] { value: T }

impl Box[@T] {
    func set(self, new_val: T) {
        self.value = new_val
    }
}

let b = Box[Int] { value: 42 }
b.set("wrong")
