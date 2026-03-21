# Dupe can be called explicitly as a method
kind Counter { n: Int }

impl Counter as Dupe {
    func dupe(self): Self {
        print("duping {self.n}")
        ret Counter { n: self.n }
    }
}

let a = Counter { n: 1 }
let b = a.dupe()
b.n = 99
print(a.n)
print(b.n)
