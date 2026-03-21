# Match is an expression — produces a value
enum Color { Red, Green, Blue }

let c = Color.Green
let name = match c {
    Red -> "red",
    Green -> "green",
    Blue -> "blue",
}
print(name)
