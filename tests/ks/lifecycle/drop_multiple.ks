# Multiple droppable values in the same scope drop in LIFO order:
# values declared later are dropped first, so anything they built on
# top of is still alive while their drop runs.
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
