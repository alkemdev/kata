# Tuple patterns destructure tuple values
let p = (1, 2, 3)
let sum = match p {
    (a, b, c) -> a + b + c,
}
print(sum)
