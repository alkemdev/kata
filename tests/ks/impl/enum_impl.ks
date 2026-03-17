enum Color { Red, Green, Blue }

impl Color {
    func is_red(self): Bool {
        ret self == Color.Red
    }
}

let c = Color.Red
print(c.is_red())
let g = Color.Green
print(g.is_red())