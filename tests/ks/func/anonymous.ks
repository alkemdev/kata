# Anonymous function expressions: `func(params) (: ret)? { body }`.
# Captures its surrounding scope just like a named func — the only
# difference is no name is hoisted, so anon funcs can't recurse by name.
let greet = func() { print("hello") }
greet()

# Typed params + return annotation.
let add = func(a: Int, b: Int): Int { ret a + b }
print(add(3, 4))

# Closure capture: outer-scope mutation via shared slot still works.
let n = 0
let bump = func() { n = n + 1 }
bump()
bump()
bump()
print(n)

# Higher-order: anon func passed as an argument and returned from another.
func apply_twice(f) { f(); f() }
apply_twice(func() { print("tick") })

func make_adder(k: Int) {
    ret func(x: Int): Int { ret x + k }
}
let add10 = make_adder(10)
print(add10(5))
