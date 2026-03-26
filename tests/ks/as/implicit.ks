# Implicit as: function with interface-typed param
kind Dog { name: Str }
kind Cat { name: Str }
type Greetable { func greet(self): Str }
impl Dog { func greet(self): Str { ret "woof" } }
impl Cat { func greet(self): Str { ret "meow" } }
impl Dog as Greetable {}
impl Cat as Greetable {}

func say(g: Greetable) {
    print(g.greet())
}

say(Dog { name: "Rex" })
say(Cat { name: "Whiskers" })
