type Pair[A, B] { fst: A, snd: B }
let p = Pair[Int, Str] { fst: 42, snd: "hello" }
print(p)
print(p.fst)
print(p.snd)
