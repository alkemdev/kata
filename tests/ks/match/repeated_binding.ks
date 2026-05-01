# Same binding name twice in a match arm pattern is a hard error
let pair = (1, 2)
print(match pair {
    (x, x) -> "eq",
    (a, b) -> "ne",
})
