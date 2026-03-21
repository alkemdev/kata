# The KataScript Zoo
# A showcase of language features

# ── Enums + match ────────────────────────────────────────────────

enum Animal {
    Cat(Str),
    Dog(Str),
    Fish,
}

func describe(a: Animal): Str {
    ret match a {
        Cat(name) -> "a cat named {name}",
        Dog(name) -> "a dog named {name}",
        Fish -> "a fish",
    }
}

print(describe(Animal.Cat("Whiskers")))
print(describe(Animal.Dog("Rex")))
print(describe(Animal.Fish))

# ── Arrays + iteration ───────────────────────────────────────────

let animals = [
    Animal.Cat("Luna"),
    Animal.Dog("Max"),
    Animal.Fish,
    Animal.Cat("Milo"),
    Animal.Dog("Bella"),
]

print("\nAll animals:")
for a in animals {
    print("  - {describe(a)}")
}

# ── Opt + ? operator ────────────────────────────────────────────

func find_cat(animals: Arr[Animal], index: Int): Str {
    let opt = animals.get(index)
    let a = match opt {
        Val(x) -> x,
        Non -> ret "not found",
    }
    ret match a {
        Cat(name) -> name,
        _ -> "not a cat",
    }
}

print("\nLooking for cats:")
print("  index 0: {find_cat(animals, 0)}")
print("  index 1: {find_cat(animals, 1)}")
print("  index 99: {find_cat(animals, 99)}")

# ── Structs + methods ────────────────────────────────────────────

kind Cage { animal: Animal, size: Int }

impl Cage {
    func label(self): Str {
        ret match self.animal {
            Cat(name) -> "Cat: {name} (size {self.size})",
            Dog(name) -> "Dog: {name} (size {self.size})",
            Fish -> "Fish (size {self.size})",
        }
    }
}

print("\nCages:")
let cages = [
    Cage { animal: Animal.Cat("Luna"), size: 3 },
    Cage { animal: Animal.Dog("Max"), size: 5 },
    Cage { animal: Animal.Fish, size: 1 },
]

for cage in cages {
    print("  [{cage.label()}]")
}

# ── Fibonacci with arrays ────────────────────────────────────────

func fib_seq(n: Int): Arr[Int] {
    let seq = [0, 1]
    let i = 2
    while i < n {
        let a = seq.get(i - 2).unwrap()
        let b = seq.get(i - 1).unwrap()
        seq.push(a + b)
        i = i + 1
    }
    ret seq
}

print("\nFibonacci:")
for n in fib_seq(10) {
    print("  {n}")
}

# ── Generic data structures ──────────────────────────────────────

kind Pair[A, B] { fst: A, snd: B }

let pairs = [
    Pair[Str, Int] { fst: "age", snd: 5 },
    Pair[Str, Int] { fst: "weight", snd: 12 },
]

print("\nPairs:")
for p in pairs {
    print("  {p.fst} = {p.snd}")
}

# ── Res[T, E] + error handling ────────────────────────────────────

func safe_divide(a: Int, b: Int): Res[Int, Str] {
    if b == 0 {
        ret Res[Int, Str].Err("division by zero")
    }
    ret Res[Int, Str].Val(a / b)
}

print("\nDivision:")
for pair in [Pair[Str, Int] { fst: "10/2", snd: 2 }, Pair[Str, Int] { fst: "10/0", snd: 0 }] {
    let result = safe_divide(10, pair.snd)
    match result {
        Val(n) -> print("  {pair.fst} = {n}"),
        Err(e) -> print("  {pair.fst} = error: {e}"),
    }
}
