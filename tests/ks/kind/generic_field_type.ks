# A struct field whose type is itself a generic application
kind Wrapper[T] { inner: Opt[T] }

let w = Wrapper[Int] { inner: Opt[Int].Val(42) }
print(w)
print(w.inner)
