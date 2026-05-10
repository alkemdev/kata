# `let <name>: <Type> = <value>` validates the value's type at runtime.
let x: Int = 42
print(x)

let s: Str = "hello"
print(s)

# Generic instantiation in the annotation: Opt[Int].
let maybe: Opt[Int] = Opt[Int].Val(7)
print(maybe.unwrap())

# Type-checks before destructuring: tuple element types still flow normally.
let pair: Tup[Int, Str] = (1, "one")
print(pair.0)
print(pair.1)
