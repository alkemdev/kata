# Specialized impl takes priority over generic for matching type
kind Wrapper[T] { val: T }

impl Wrapper[@T] {
    func describe(self): Str {
        ret "generic"
    }
}

impl Wrapper[Int] {
    func describe(self): Str {
        ret "specialized"
    }
}

let w = Wrapper[Int] { val: 42 }
print(w.describe())

let w2 = Wrapper[Str] { val: "hi" }
print(w2.describe())
