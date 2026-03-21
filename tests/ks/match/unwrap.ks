# Opt.unwrap uses match internally
let a = [10, 20, 30]
print(a.get(0).unwrap())
print(a.get(1).unwrap())
print(a.get(99).unwrap_or(0))
