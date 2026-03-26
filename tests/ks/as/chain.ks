type Printable { func show(self): Str }
kind A { val: Int }
kind B { val: Str }
impl A { func show(self): Str { ret "{self.val}" } }
impl B { func show(self): Str { ret self.val } }
impl A as Printable {}
impl B as Printable {}

func display(p: Printable) { print(p.show()) }
display(A { val: 42 })
display(B { val: "hello" })
