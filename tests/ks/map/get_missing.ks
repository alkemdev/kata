# Map: bracket access on missing key panics
let m = Map[Str, Int].new()
print(m["nope"])
