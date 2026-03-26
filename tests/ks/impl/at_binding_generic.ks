# @T syntax declares a type parameter — works like the old impl Foo[T]
kind Pair[A, B] { fst: A, snd: B }

impl Pair[@A, @B] {
    func first(self): A {
        ret self.fst
    }
    func second(self): B {
        ret self.snd
    }
}

let p = Pair[Int, Str] { fst: 42, snd: "hello" }
print(p.first())
print(p.second())
