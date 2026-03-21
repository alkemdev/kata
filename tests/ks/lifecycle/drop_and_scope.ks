# Reassignment drops old value, scope exit drops final value
kind Resource { id: Int }

impl Resource as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

func test() {
    let r = Resource { id: 1 }
    r = Resource { id: 2 }
    print("after reassign")
}

test()
print("done")
