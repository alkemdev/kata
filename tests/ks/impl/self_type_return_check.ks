# Self as return type is type-checked
kind Dog { name: Str }
kind Cat { name: Str }

impl Dog {
    func try_wrong(self): Self {
        ret Cat { name: "not a dog" }
    }
}

let d = Dog { name: "Rex" }
d.try_wrong()
