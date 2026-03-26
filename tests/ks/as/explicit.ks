# Explicit as: create an interface view
kind Dog { name: Str }
type Greetable { func greet(self): Str }
impl Dog { func greet(self): Str { ret "woof from {self.name}" } }
impl Dog as Greetable {}

let d = Dog { name: "Rex" }
let g = d as Greetable
print(g.greet())
print(typeof(g))
