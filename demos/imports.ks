# Imports — qualified vs selective access to standard modules
#
# The prelude auto-loads `core` and `dsa.{Arr, ArrIter, Slot, Map, MapIter}`.
# Anything outside that — e.g. `mem.Ptr`, `mem.alloc`, `mem.heap` — needs an
# explicit `import` to come into scope.

# ── 1. Scoped import ───────────────────────────────────────────────
# `import mem` binds `mem` as a module value. Members are reached via
# qualified names: `mem.Ptr`, `mem.heap`, `mem.alloc(...)`.

import mem

# Native intrinsic — must be in `unsafe` because it touches raw memory.
let raw = unsafe { mem.alloc(4) }
unsafe { mem.write(raw, 0, 7) }
unsafe { mem.write(raw, 1, 11) }
print("scoped: mem.read(0) = {unsafe { mem.read(raw, 0) }}")
print("scoped: mem.read(1) = {unsafe { mem.read(raw, 1) }}")
unsafe { mem.free(raw) }

# Types live under the same qualified path. `mem.Ptr` is the type;
# constructing one wraps a RawPtr.
print("scoped: type mem.Ptr = {mem.Ptr}")
print("scoped: type mem.Buf = {mem.Buf}")

# The `heap` allocator is a value (a HeapAllocator singleton), reached
# the same way through `mem.heap`.
let block = mem.heap.make(2)
let p = mem.Ptr[Int] { raw: block }
p.write(0, 100)
p.write(1, 200)
print("scoped: heap-backed Ptr[Int] = ({p.read(0)}, {p.read(1)})")
mem.heap.free(block)

# ── 2. Selective import ────────────────────────────────────────────
# `import mem.{Ptr, Buf}` pulls just those names into the current
# scope, so we can write `Ptr` directly without the `mem.` prefix.
# `mem` itself remains usable from section 1.

import mem.{Ptr, Buf, heap}

let cap = 3
let buf = Buf[Int] { ptr: Ptr[Int] { raw: heap.make(cap) }, cap: cap }
buf.write(0, 1)
buf.write(1, 4)
buf.write(2, 9)
print("selective: Buf[Int] = ({buf.read(0)}, {buf.read(1)}, {buf.read(2)})")
# Buf has a Drop impl, so we let scope exit free its backing store.

# ── 3. Nested module access ────────────────────────────────────────
# `Arr` is in the prelude — no import needed.
let xs = Arr[Int].new()
xs.push(10)
xs.push(20)
xs.push(30)
print("prelude:   Arr[Int] len = {xs.len}, head = {xs.get(0).unwrap()}")

# `import dsa` exposes the whole dsa namespace. Even though `Arr` is
# already in scope from the prelude, `dsa.Arr` still resolves to the
# same type via the qualified path.
import dsa

let ys = dsa.Arr[Str].new()
ys.push("alpha")
ys.push("beta")
print("nested:    dsa.Arr[Str].get(1) = {ys.get(1).unwrap()}")

# ── 4. Without import — would-be failure ───────────────────────────
# Everything `mem` exposes (Allocator, HeapAllocator, etc.) is hidden
# from a fresh scope until `import` brings it in. The lines below
# would error if uncommented — `mem` would not be in scope, so
# `mem.Allocator` has nowhere to resolve.
#
#   # (no `import mem` yet)
#   let a: mem.Allocator = mem.heap     # ModuleNoExport: <root> has no `mem`
#
# Selective imports are similarly strict: `import mem.{NotAThing}`
# fails immediately because the module has no such export:
#
#   import mem.{NotAThing}               # ModuleNoExport: mem has no `NotAThing`
#
# The interpreter resolves segment by segment and points the error at
# the first unknown name, so typos surface with a precise span.

print("ok")
