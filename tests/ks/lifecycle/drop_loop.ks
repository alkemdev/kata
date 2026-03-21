# Loop variable is dropped each iteration
kind Resource { id: Int }

impl Resource as Drop {
    func drop(self) {
        print("dropped {self.id}")
    }
}

let i = 0
while i < 3 {
    let r = Resource { id: i }
    i = i + 1
}
print("done")
