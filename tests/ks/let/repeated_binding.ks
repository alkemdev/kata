# Same binding name twice in a let pattern is a hard error
let pair = (1, 2)
let (x, x) = pair
print(x)
