# Specialized impl can satisfy an interface
kind Wrapper[T] { val: T }

type Describable {
    func describe(self): Str
}

impl Wrapper[Int] as Describable {
    func describe(self): Str {
        ret "int: {self.val}"
    }
}

let w = Wrapper[Int] { val: 42 }
print(w.describe())
