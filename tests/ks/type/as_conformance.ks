type Greetable {
    func greet(self): Str
}

kind Dog { name: Str }

impl Dog as Greetable {
    func greet(self): Str {
        ret "woof from " + self.name
    }
}

let d = Dog { name: "Rex" }
print(d.greet())