# Multiple droppable values in the same scope
kind Resource { id: Int }

impl Resource as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

func test() {
    let a = Resource { id: 1 }
    let b = Resource { id: 2 }
    let c = Resource { id: 3 }
    print("all created")
}

test()
print("done")
