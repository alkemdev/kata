kind Point { x: Int, y: Int }

impl Point {
    func move_right(self) {
        self.x = self.x + 1
    }
}

let p = Point { x: 0, y: 0 }
p.move_right()
p.move_right()
print(p.x)