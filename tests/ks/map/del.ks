# Map: delete entries
let m = Map[Str, Int].new()
m.set("a", 1)
m.set("b", 2)
print(m.del("a"))
print(m.del("missing"))
print(m.len())
print(m.has("a"))
print(m.has("b"))
