# Iterating the same collection twice
kind Pair { a: Int, b: Int }
kind PairIter { vals: Pair, idx: Int }

impl Pair {
    func to_iter(self): PairIter {
        ret PairIter { vals: self, idx: 0 }
    }
}

impl PairIter {
    func next(self): Opt[Int] {
        if self.idx == 0 {
            self.idx = 1
            ret Opt[Int].Val(self.vals.a)
        }
        if self.idx == 1 {
            self.idx = 2
            ret Opt[Int].Val(self.vals.b)
        }
        ret Opt[Int].Non
    }
}

let p = Pair { a: 10, b: 20 }

print("first:")
for x in p {
    print(x)
}

print("second:")
for x in p {
    print(x)
}
