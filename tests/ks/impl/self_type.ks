# Self resolves to the implementing type inside impl blocks
kind Point { x: Int, y: Int }

impl Point {
    func translate(self, dx: Int, dy: Int): Self {
        ret Point { x: self.x + dx, y: self.y + dy }
    }

    func zero(self): Self {
        ret Point { x: 0, y: 0 }
    }
}

let p = Point { x: 1, y: 2 }
let q = p.translate(3, 4)
print(q)

let z = p.zero()
print(z)
