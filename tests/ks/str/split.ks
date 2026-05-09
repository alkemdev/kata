# Str.split returns a real Arr[Str] — iterable, indexable, length-aware.
let s = "alpha,beta,gamma,delta"
let parts = s.split(",")
print(parts.len)

for p in parts {
    print(p)
}

# Empty delimiter results: split on empty string yields one part per char.
let chars = "abc".split("")
print(chars.len)

# Delimiter not present: yields one element (the whole string).
let one = "no-delim-here".split("|")
print(one.len)
