# Res methods: unwrap, unwrap_or, unwrap_err, is_val, is_err
let ok = Res[Int, Str].Val(42)
let err = Res[Int, Str].Err("bad")

print(ok.unwrap())
print(ok.unwrap_or(0))
print(ok.is_val())
print(ok.is_err())

print(err.unwrap_or(0))
print(err.unwrap_err())
print(err.is_val())
print(err.is_err())
