# Map: iteration yields (key, value) pairs
let m = Map[Int, Str].new()
m.set(1, "a")
m.set(2, "b")
let keys = Arr[Int].new()
for entry in m {
    keys.push(entry.0)
}
print(keys.len)
