# Method dispatch — a tour of `impl` blocks
#
# KataScript associates functions with types via `impl` blocks. Methods are
# looked up in a per-type table keyed by handle, with fallback from instance
# type (e.g. Box[Int]) to base type (Box). This demo walks each piece.

# ── 1. Basic impl ───────────────────────────────────────────────
# A `kind` is a concrete product type (struct). An `impl` block attaches
# methods. `self` is the explicit receiver — a regular first parameter that
# the caller doesn't pass when using dot syntax.

kind Point { x: Int, y: Int }

impl Point {
    func magnitude_sq(self): Int {
        ret self.x * self.x + self.y * self.y
    }

    func to_str(self): Str {
        ret "({self.x}, {self.y})"
    }
}

let p = Point { x: 3, y: 4 }
print("--- 1. Basic impl ---")
print("p = {p.to_str()}")
print("|p|^2 = {p.magnitude_sq()}")


# ── 2. Generic impl ─────────────────────────────────────────────
# `impl Box[@T]` binds T as a type parameter (the @ sigil). The methods
# work for any T — calls on Box[Int], Box[Str], Box[Bool] all dispatch here
# unless a more specific impl exists.

kind Box[T] { val: T }

impl Box[@T] {
    func get(self): T {
        ret self.val
    }

    func tag(self): Str {
        ret "generic Box"
    }
}

let bi = Box[Int] { val: 42 }
let bs = Box[Str] { val: "hi" }
print("\n--- 2. Generic impl ---")
print("bi.get()  = {bi.get()}")
print("bs.get()  = {bs.get()}")
print("bi.tag()  = {bi.tag()}")
print("bs.tag()  = {bs.tag()}")


# ── 3. Specialized impl ─────────────────────────────────────────
# `impl Box[Int]` overrides the generic methods for one concrete
# instantiation. Lookup checks the instance type first; only if nothing is
# found there does it fall through to the base (generic) impl.

impl Box[Int] {
    func tag(self): Str {
        ret "Box[Int] specialized"
    }

    func doubled(self): Int {
        ret self.val * 2
    }
}

let si = Box[Int] { val: 21 }
let ss = Box[Str] { val: "hi" }
print("\n--- 3. Specialized impl ---")
print("si.tag()  = {si.tag()}")     # specialized
print("ss.tag()  = {ss.tag()}")     # generic — no Box[Str] impl exists
print("si.doubled() = {si.doubled()}")


# ── 4. Mutable self (copy-in copy-out) ──────────────────────────
# Methods can assign to `self`'s fields. The interpreter snapshots self
# before the call and writes the final value back to the receiver variable
# after. This works for direct `var.method()` receivers.

kind Counter { value: Int }

impl Counter {
    func tick(self) {
        self.value = self.value + 1
    }

    func add(self, n: Int) {
        self.value = self.value + n
    }
}

let c = Counter { value: 0 }
c.tick()
c.tick()
c.add(10)
print("\n--- 4. Mutable self ---")
print("after 2 ticks + add(10): c.value = {c.value}")


# ── 5. Static methods ───────────────────────────────────────────
# A method without a `self` parameter is a static method, called on the
# type itself: `TypeName.fn(args)`. Useful for constructors and factories.

impl Point {
    func origin(): Point {
        ret Point { x: 0, y: 0 }
    }

    func unit_x(): Point {
        ret Point { x: 1, y: 0 }
    }
}

let o = Point.origin()
let ux = Point.unit_x()
print("\n--- 5. Static methods ---")
print("origin = {o.to_str()}")
print("unit_x = {ux.to_str()}")


# ── 6. Base-type fallback ───────────────────────────────────────
# Dispatch goes: instance type (Box[Str]) -> base type (Box). The Box[Str]
# variable below has no specialized impl, so `tag()` resolves on the base
# type's @T impl. No `impl Box[Str]` is needed for Box[Str] to have methods.
#
# Same idea for `get()` — only defined on the generic, but works on every
# instance because base-type fallback always finds it.

let bx_str = Box[Str] { val: "fallback works" }
let bx_bool = Box[Bool] { val: true }
print("\n--- 6. Base-type fallback ---")
print("bx_str.tag()  = {bx_str.tag()}")     # falls back to Box[@T].tag
print("bx_bool.tag() = {bx_bool.tag()}")    # falls back to Box[@T].tag
print("bx_str.get()  = {bx_str.get()}")     # generic only
print("bx_bool.get() = {bx_bool.get()}")    # generic only

# Sanity: Box[Int] still finds the specialized override, not the fallback.
let bx_int = Box[Int] { val: 7 }
print("bx_int.tag()  = {bx_int.tag()}")     # Box[Int] wins over Box[@T]
