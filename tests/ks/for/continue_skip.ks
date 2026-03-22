# cont skips the current iteration
kind Counter { n: Int }
kind CounterIter { current: Int, max: Int }

impl Counter {
    func to_iter(self): CounterIter {
        ret CounterIter { current: 0, max: self.n }
    }
}

impl CounterIter {
    func next(self): Opt[Int] {
        if self.current >= self.max {
            ret Opt[Int].Non
        }
        let val = self.current
        self.current = self.current + 1
        ret Opt[Int].Val(val)
    }
}

for x in Counter { n: 5 } {
    if x == 2 || x == 4 {
        cont
    }
    print(x)
}
