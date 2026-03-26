# impl Foo[T] without @ should error — T is not a known type
kind Box[T] { val: T }

impl Box[T] {
    func get(self): T { ret self.val }
}
