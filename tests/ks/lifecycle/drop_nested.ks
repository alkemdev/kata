# Struct fields that implement Drop are recursively dropped
kind Inner { id: Int }
kind Outer { inner: Inner }

impl Inner as Drop {
    func drop(self) {
        print("inner dropped {self.id}")
    }
}

func test() {
    let o = Outer { inner: Inner { id: 1 } }
    print("created")
}

test()
print("done")
