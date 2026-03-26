# Static methods: no self parameter, called on the type value
kind Point { x: Int, y: Int }

impl Point {
    func origin(): Point {
        ret Point { x: 0, y: 0 }
    }

    func dist(self): Int {
        ret self.x + self.y
    }
}

let p = Point.origin()
print(p.x)
print(p.y)
print(p.dist())
