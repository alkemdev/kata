# Res.unwrap on Err panics
let err = Res[Int, Str].Err("bad")
err.unwrap()
