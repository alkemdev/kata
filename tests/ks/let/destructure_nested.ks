# let with nested tuple destructure
let nested = ((1, 2), 3)
let ((x, y), z) = nested
print(x)
print(y)
print(z)
