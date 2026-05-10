# Stress 01: Deeply nested generics — Map[Str, Arr[Tup[Int, Opt[Str]]]]
# Tests construction, population, lookup, and iteration over a complex
# parametric type composition.

let m = Map[Str, Arr[Tup[Int, Opt[Str]]]].new()

let arr_a = Arr[Tup[Int, Opt[Str]]].new()
arr_a.push((1, Opt[Str].Val("one")))
arr_a.push((2, Opt[Str].Non))
arr_a.push((3, Opt[Str].Val("three")))

let arr_b = Arr[Tup[Int, Opt[Str]]].new()
arr_b.push((10, Opt[Str].Non))
arr_b.push((20, Opt[Str].Val("twenty")))

m.set("a", arr_a)
m.set("b", arr_b)

print(m.len())
print(m.has("a"))
print(m.has("missing"))

# Iterate over the map. The outer iterator yields Tup[Str, Arr[...]].
for entry in m {
    let key = entry.0
    let arr = entry.1
    print("key={key} len={arr.len}")
    for inner in arr {
        let n = inner.0
        let s = match inner.1 {
            Val(x) -> x,
            Non() -> "nil",
        }
        print("  ({n}, {s})")
    }
}
