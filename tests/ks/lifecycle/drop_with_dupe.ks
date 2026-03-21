# Drop and Dupe work together on the same type
# Note: calling a.dupe() creates a parameter clone of a that is
# dropped when the method returns, producing an extra drop.
kind Handle { id: Int }

impl Handle as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

impl Handle as Dupe {
    func dupe(self): Self {
        ret Handle { id: self.id + 100 }
    }
}

func test() {
    let a = Handle { id: 1 }
    let b = a.dupe()
    print("a={a.id} b={b.id}")
}

test()
print("done")
