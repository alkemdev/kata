# Drop fires when a variable is reassigned
kind Resource { id: Int }

impl Resource as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

let r = Resource { id: 1 }
r = Resource { id: 2 }
print("after reassign")
