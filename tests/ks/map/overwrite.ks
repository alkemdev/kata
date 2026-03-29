# Map: overwriting a key updates the value
let m = Map[Str, Int].new()
m.set("a", 1)
m.set("a", 99)
print(m["a"])
print(m.len())
