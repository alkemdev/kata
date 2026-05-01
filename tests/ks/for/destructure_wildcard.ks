# wildcard in for loop pattern — only need values
let m = Map[Str, Int].new()
m.set("x", 10)
m.set("y", 20)

let sum = 0
for (_, v) in m {
    sum = sum + v
}
print(sum)
