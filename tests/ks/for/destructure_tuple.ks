# for loop destructures yielded tuple values
let m = Map[Str, Int].new()
m.set("a", 1)
m.set("b", 2)
m.set("c", 3)

let total = 0
for (k, v) in m {
    total = total + v
}
print(total)
