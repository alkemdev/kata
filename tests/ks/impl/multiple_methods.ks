kind Point { x: Int, y: Int }

impl Point {
    func get_x(self): Int {
        ret self.x
    }

    func get_y(self): Int {
        ret self.y
    }
}

let p = Point { x: 10, y: 20 }
print(p.get_x())
print(p.get_y())