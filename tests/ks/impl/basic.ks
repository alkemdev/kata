kind Point { x: Int, y: Int }

impl Point {
    func sum(self): Int {
        ret self.x + self.y
    }
}

let p = Point { x: 3, y: 4 }
print(p.sum())