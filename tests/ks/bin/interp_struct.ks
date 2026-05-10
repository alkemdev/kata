# b"..." interpolation with a kind value — uses Display output
kind Point { x: Float, y: Float }
let p = Point { x: 1.5, y: 2.5 }
let b = b"pt={p}"
print(b)
