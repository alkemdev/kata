# Static methods on enums — variants take priority, static as fallback
enum Color { Red, Green, Blue }

impl Color {
    func default(): Color {
        ret Color.Red
    }
}

print(Color.default())
print(Color.Red)
