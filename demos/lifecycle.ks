# A Drop / Copy / Dupe lifecycle showcase.
#
# Three interfaces from std/core/lifecycle.ks govern how values come and
# go in KataScript:
#
#   - Drop  — `func drop(self)` runs when a value leaves scope (LET-bound
#             slot popped, reassigned, struct field collected).
#   - Copy  — `func copy(self): Self` is an explicit "give me an
#             independent value" method. KataScript's *default* assignment
#             already copies values structurally; Copy is a marker
#             interface for types that opt in to a uniform `.copy()`.
#   - Dupe  — `func dupe(self): Self` is the explicit "clone" hook,
#             distinct from Copy by convention (Copy = cheap, Dupe =
#             potentially expensive deep duplicate). Both are user-driven.
#
# See docs/spec/type-system.md and std/core/lifecycle.ks for the
# canonical definitions.

# ── 1. Drop fires on scope exit ─────────────────────────────────
#
# A value of a Drop-implementing type has its `drop` method invoked
# the moment its slot is freed. Inside `make`, `r` is bound by `let`
# in a fresh function frame; when the function returns, the frame
# is popped and Drop fires.

print("── 1. Drop fires on scope exit ──")

kind R { id: Int }

impl R as Drop {
    func drop(self) {
        print("  drop R{self.id}")
    }
}

func make() {
    let r = R { id: 1 }
    print("  inside make, r.id = {r.id}")
}
print("  before make()")
make()
print("  after make()")

# A `with` block also gets its own scope frame, so Drop fires when
# the block ends — useful for explicit "destructor" patterns.

print("\n  (with-block scope)")
print("  before with")
with r = R { id: 7 } {
    print("  inside with, r.id = {r.id}")
}
print("  after with")

# ── 2. Drop ordering: insertion order, not LIFO ─────────────────
#
# Many Rust readers expect LIFO (newest-first) drop on scope exit.
# KataScript drops in *insertion order* — the order names were bound
# in the frame. The actual ordering surfaces on multiple sibling
# bindings:

print("\n── 2. Drop ordering ──")

func three() {
    let a = R { id: 100 }
    let b = R { id: 200 }
    let c = R { id: 300 }
    print("  all three created (a, b, c)")
}
three()
# Expect: drop R100, drop R200, drop R300 — FIFO (insertion order).

# ── 3. Reassignment drops the old value eagerly ─────────────────
#
# Assignment to an existing binding drops the old occupant before
# installing the new one. The final occupant drops on scope exit.

print("\n── 3. Reassignment drops the old value ──")

func reassign_demo() {
    let r = R { id: 10 }
    print("  before reassign")
    r = R { id: 20 }
    print("  after reassign")
}
reassign_demo()

# ── 4. Default assignment already makes an independent copy ─────
#
# In KataScript, `let b = a` for a kind value copies the struct
# field-by-field. Mutating `b` does not affect `a`. This holds
# whether or not the type implements `Copy` — Copy is a marker /
# uniform method, not a runtime gate.

print("\n── 4. Default assignment is by-value ──")

kind Point { x: Int, y: Int }

let a = Point { x: 1, y: 2 }
let b = a            # independent copy, no `.copy()` needed
b.x = 99
print("  after mutating b: a = ({a.x}, {a.y})  b = ({b.x}, {b.y})")

# Opting into Copy gives you a uniform `.copy()` method — useful
# when generic code needs to ask any value for a fresh copy without
# knowing the concrete type. The runtime semantics don't change.

impl Point as Copy {
    func copy(self): Self {
        ret Point { x: self.x, y: self.y }
    }
}

let p = Point { x: 5, y: 6 }
let q = p.copy()
q.x = 500
print("  via .copy():        p = ({p.x}, {p.y})  q = ({q.x}, {q.y})")

# ── 5. Dupe — explicit, side-effect-aware duplication ───────────
#
# Dupe is the convention for "I want a fresh independent value, and
# I want the call to be visible." Unlike a passive `let b = a` copy,
# Dupe runs your method body — handy for logging, allocating fresh
# resources, or stamping a different identity on the duplicate.

print("\n── 5. Dupe ──")

kind Handle { id: Int }

impl Handle as Drop {
    func drop(self) {
        print("  drop Handle{self.id}")
    }
}

impl Handle as Dupe {
    func dupe(self): Self {
        print("  duping Handle{self.id} -> Handle{self.id + 1000}")
        ret Handle { id: self.id + 1000 }
    }
}

func dupe_demo() {
    let h = Handle { id: 1 }
    let h2 = h.dupe()
    print("  h.id = {h.id}, h2.id = {h2.id}")
    # Both h and h2 will Drop on scope exit, in the order they were
    # bound (h first, then h2). The dupe call itself produces a
    # printed "duping ..." line above, but no extra drops — the
    # method's `self` slot is not Drop-fired (current behavior;
    # see tests/ks/lifecycle/drop_with_dupe.ks).
}
dupe_demo()

# ── 6. Closures + Drop: capture is by slot, not by ownership ────
#
# KataScript closures capture by slot reference — the closure can
# read and write the slot after the defining scope returns. But
# Drop runs eagerly when the *defining frame* is popped: every
# slot in that frame has its value dropped, regardless of whether
# the slot is also reachable through a returned closure.
#
# So a closure that captures a Drop value can still resolve the
# field through the captured slot, but the Drop side-effects have
# already fired by the time the closure runs. (See
# tests/ks/func/closure_drop_capture.ks — this is locked-in
# current behavior, intentionally not ref-count-aware.)

print("\n── 6. Closure capture and eager Drop ──")

func make_user(): Func {
    let h = Handle { id: 42 }
    func use_it() {
        print("  closure sees h.id = {h.id}")
    }
    print("  about to leave make_user — watch the drop")
    ret use_it
}

let f = make_user()
print("  make_user returned — drop already fired above")
f()
print("  (closure body still resolves h.id via the captured slot)")

print("\n── done ──")
