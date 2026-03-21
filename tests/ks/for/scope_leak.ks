# Loop variable doesn't leak out of the for block
kind One { n: Int }
kind OneIter { done: Bool }

impl One {
    func to_iter(self): OneIter {
        ret OneIter { done: false }
    }
}

impl OneIter {
    func next(self): Opt[Int] {
        if self.done {
            ret Opt[Int].Non
        }
        self.done = true
        ret Opt[Int].Val(42)
    }
}

for x in One { n: 0 } {
    print(x)
}
print(typeof(x))
