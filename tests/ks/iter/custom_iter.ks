// A custom iterable that counts down from n to 1
kind Countdown { n: Int }
kind CountdownIter { current: Int }

impl Countdown {
    func to_iter(self): CountdownIter {
        ret CountdownIter { current: self.n }
    }
}

impl CountdownIter {
    func next(self): Opt[Int] {
        if self.current > 0 {
            let val = self.current
            self.current = self.current - 1
            ret Opt[Int].Some(val)
        }
        ret Opt[Int].None
    }
}

for x in Countdown { n: 3 } {
    print(x)
}