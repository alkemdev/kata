kind Point { x: Int, y: Int }

impl Point {
    func get_x(self): Int {
        ret self.x
    }
}

impl Point {
    func get_y(self): Int {
        ret self.y
    }
}

let p = Point { x: 5, y: 6 }
print(p.get_x())
print(p.get_y())