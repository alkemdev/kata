# Nested for loops
kind Range { n: Int }
kind RangeIter { current: Int, max: Int }

impl Range {
    func to_iter(self): RangeIter {
        ret RangeIter { current: 0, max: self.n }
    }
}

impl RangeIter {
    func next(self): Opt[Int] {
        if self.current >= self.max {
            ret Opt[Int].Non
        }
        let val = self.current
        self.current = self.current + 1
        ret Opt[Int].Val(val)
    }
}

for i in Range { n: 3 } {
    for j in Range { n: 2 } {
        print("{i},{j}")
    }
}
