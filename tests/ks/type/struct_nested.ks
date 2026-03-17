type Point { x: Float, y: Float }
type Line { start: Point, end: Point }
let l = Line { start: Point { x: 0.5, y: 1.5 }, end: Point { x: 3.5, y: 4.5 } }
print(l.start.x)
print(l.end.y)
