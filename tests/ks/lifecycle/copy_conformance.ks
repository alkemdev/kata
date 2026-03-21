# Copy conformance succeeds when the type has a copy method
kind Point { x: Int, y: Int }

impl Point as Copy {
    func copy(self): Self {
        ret Point { x: self.x, y: self.y }
    }
}

let a = Point { x: 1, y: 2 }
let b = a.copy()
b.x = 99
print(a.x)
print(b.x)
