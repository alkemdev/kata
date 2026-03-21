# Drop is called when a value goes out of scope
kind Resource { id: Int }

impl Resource as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

func test() {
    let r = Resource { id: 1 }
    print("inside scope")
}

test()
print("after scope")
