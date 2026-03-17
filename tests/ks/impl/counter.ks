kind Counter { value: Int }

impl Counter {
    func increment(self) {
        self.value = self.value + 1
    }

    func get(self): Int {
        ret self.value
    }
}

let c = Counter { value: 0 }
c.increment()
c.increment()
c.increment()
print(c.get())