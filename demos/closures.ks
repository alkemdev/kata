# A closures showcase.
#
# KataScript closures use slot-based capture: a binding is a shared,
# interior-mutable cell, and a closure captures the scope chain by
# `Arc`-cloning the frames it sees. Mutating a name through one path
# (the outer scope, a returned closure, a sibling closure) is visible
# through every other path that points at the same slot.
#
# See docs/spec/closures.md for the full conceptual model.

# ── 1. Counter factory ───────────────────────────────────────────
#
# `make_counter` builds a fresh `count` slot on every call, then
# returns a closure that increments and reads it. The returned
# closure is the only handle to that count — it's private state.

func make_counter(): Func {
    let count = 0
    func step(): Int {
        count = count + 1
        ret count
    }
    ret step
}

print("── 1. Counter factory ──")
let c = make_counter()
print("first call:  {c()}")
print("second call: {c()}")
print("third call:  {c()}")

# ── 2. Two counters with independent state ──────────────────────
#
# Each call to `make_counter` runs the body in a new frame, so
# `let count = 0` mints a brand-new slot. Two products of the
# same factory share *no* state — the only thing they share is
# the source of the function body.

print("\n── 2. Independent counters ──")
let a = make_counter()
let b = make_counter()
print("a: {a()}  b: {b()}")
print("a: {a()}  b: {b()}")
print("a: {a()}  a: {a()}")
print("b advances on its own: {b()}")

# ── 3. Closure that mutates outer scope ─────────────────────────
#
# Assignment inside a closure walks the scope chain and writes
# through the existing slot. Here `bump` and `reset` both close
# over the same top-level `total` slot, so a write through one is
# observable through the other and through the outer scope.

print("\n── 3. Mutating outer scope ──")
let total = 0
func bump(by: Int) {
    total = total + by
}
func reset() {
    total = 0
}

bump(5)
bump(7)
print("after two bumps:   total = {total}")
bump(10)
print("after another bump: total = {total}")
reset()
print("after reset:       total = {total}")

# ── 4. Closure capturing a struct ───────────────────────────────
#
# Structs are captured by the same slot mechanism. The closure
# holds a reference to the slot for `acct`, so `.balance` updates
# through `deposit` and `withdraw` are visible to the outer scope
# and to every closure that captured the same slot.

print("\n── 4. Closure over a struct ──")
kind Account { owner: Str, balance: Int }

let acct = Account { owner: "Ada", balance: 100 }

func deposit(amount: Int) {
    acct.balance = acct.balance + amount
}
func withdraw(amount: Int) {
    acct.balance = acct.balance - amount
}
func show() {
    print("  {acct.owner}: balance = {acct.balance}")
}

show()
deposit(50)
show()
withdraw(30)
show()
deposit(200)
show()

# ── 5. Closures over iteration ──────────────────────────────────
#
# Arr doesn't ship a `map`/`filter` in stdlib (no higher-order Arr
# methods yet), but we can drive iteration manually with a
# closure carrying its own state. Here `running_sum` is built as
# a factory product: each call accumulates and returns the sum
# so far, threading state through what would otherwise be a fold.

print("\n── 5. Closures driving iteration ──")
func make_running_sum(): Func {
    let sum = 0
    func add(x: Int): Int {
        sum = sum + x
        ret sum
    }
    ret add
}

let nums = [3, 1, 4, 1, 5, 9, 2, 6]
let running = make_running_sum()
for n in nums {
    print("  +{n} -> running sum = {running(n)}")
}

# A second closure flavor: a parameterized transformer. `factor`
# is captured at factory-call time; the returned closure stamps
# every input with that captured multiplier.

func make_scaler(factor: Int): Func {
    func scale(x: Int): Int {
        ret x * factor
    }
    ret scale
}

print("\n  scaled views of [1, 2, 3, 4]:")
let by_two   = make_scaler(2)
let by_ten   = make_scaler(10)
let small    = [1, 2, 3, 4]
for n in small {
    print("    {n} -> x2 = {by_two(n)},  x10 = {by_ten(n)}")
}
