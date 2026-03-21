# Pair[Int, Str] must reject Str in fst (param 0 = Int)
kind Pair[A, B] { fst: A, snd: B }
let p = Pair[Int, Str] { fst: "wrong", snd: "ok" }
