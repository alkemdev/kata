# Map: reinserting after delete uses tombstone correctly
let m = Map[Str, Int].new()
m.set("a", 1)
m.del("a")
m.set("a", 2)
print(m["a"])
print(m.len())
