# Generic impl: methods defined on Wrapper[T] work on Wrapper[Int]
kind Wrapper[T] { inner: Opt[T] }

impl Wrapper[T] {
    func get_or(self, default: T): T {
        ret default
    }
}

let w = Wrapper[Int] { inner: Opt[Int].Some(42) }
print(w.get_or(0))
print(w.inner)

let w2 = Wrapper[Str] { inner: Opt[Str].None }
print(w2.get_or("fallback"))
