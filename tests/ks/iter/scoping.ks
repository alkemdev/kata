kind IntRange { start: Int, end: Int }
kind IntRangeIter { current: Int, end: Int }

impl IntRange {
    func to_iter(self): IntRangeIter {
        ret IntRangeIter { current: self.start, end: self.end }
    }
}

impl IntRangeIter {
    func next(self): Opt[Int] {
        if self.current < self.end {
            let val = self.current
            self.current = self.current + 1
            ret Opt[Int].Some(val)
        }
        ret Opt[Int].None
    }
}

let x = 99
for x in IntRange { start: 0, end: 2 } {
    print(x)
}
print(x)