type Greetable {
    func greet(self): Str
}

kind Cat { name: Str }

impl Cat as Greetable {
    func purr(self): Str {
        ret "purr"
    }
}