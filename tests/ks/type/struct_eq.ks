type Point { x: Float, y: Float }
let a = Point { x: 1.5, y: 2.5 }
let b = Point { x: 1.5, y: 2.5 }
let c = Point { x: 3.5, y: 4.5 }
print(a == b)
print(a == c)
print(a != c)
