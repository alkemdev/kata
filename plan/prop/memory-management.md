# Decision: memory management architecture
**ID:** memory-management
**Status:** open
**Date opened:** 2026-03-21
**Date done:** —
**Affects:** interpreter, syntax, stdlib, types

## Question
How should KataScript handle heap allocation, growable storage, and the safety boundary around raw memory operations?

## Context
KataScript needs collection types (`Arr[T]`, `Map[K,V]`, etc.) but has no mechanism for variable-length heap-allocated storage. All current values are either fixed-size prims (Int, Bool, Str) or fixed-structure composites (kind fields, enum variants). None can express "a growable sequence of N items."

The stdlib philosophy says collections should be KS types, not runtime primitives. This means KS needs access to allocation primitives that the runtime provides — but those primitives are inherently unsafe (dangling pointers, double frees, out-of-bounds access). The language needs a safety boundary.

The proposed architecture is three layers:

```
std.dsa.Arr[T]       — user-facing, safe, iterable
  └─ std.mem.Buf[T]  — typed buffer with capacity + len, bounds-checked
      └─ std.mem.Ptr[T]  — raw allocation handle, unsafe ops only
          └─ runtime      — actual Vec<Value> in Rust
```

## Alternatives

### Option A: Single-layer — Arr as a prim Value variant
Add `Value::Arr { type_id, elements: Vec<Value> }` directly. All operations are builtins.
**Pros:** Simplest. No new concepts. Works immediately.
**Cons:** Not self-hosted. No reuse for Map/Set/other collections. Doesn't establish the allocation pattern. Violates the stdlib philosophy.

### Option B: Two-layer — Buf[T] + Arr[T]
`Buf[T]` is a prim value wrapping `Vec<Value>` with capacity tracking. `Arr[T]` is a `kind` wrapping a `Buf[T]`. No raw pointer layer.
**Pros:** Simpler than three layers. Buf provides the safety boundary directly. Arr is pure KS.
**Cons:** Buf conflates allocation with bounds checking. Can't build other data structures (ring buffers, hash tables) that need raw allocation without bounds-checked access.

### Option C: Three-layer — Ptr[T] + Buf[T] + Arr[T]
`Ptr[T]` is an opaque handle to runtime-managed storage. Operations on Ptr are unsafe (no bounds checking, no type checking). `Buf[T]` wraps Ptr in a safe interface with capacity/len tracking. `Arr[T]` wraps Buf with the user-facing API.
**Pros:** Maximum reuse — Ptr backs all collection types. Clean safety boundary. Mirrors Rust's NonNull → RawVec → Vec. Each layer has a single responsibility.
**Cons:** Three layers of abstraction for "I want a list." Ptr[T] is powerful but dangerous — needs unsafe to gate it. More implementation work.

## Discussion

### What is `Ptr[T]`?

A prim value: `Value::Ptr(PtrId)` where `PtrId` indexes into a runtime-managed allocation table (`Vec<Vec<Value>>`). The runtime provides intrinsic functions, callable only inside `unsafe` blocks:

- `__ptr_alloc(capacity: Int): Ptr[T]` — allocate storage for N elements
- `__ptr_dealloc(ptr: Ptr[T])` — free the storage (must not use ptr after this)
- `__ptr_read(ptr: Ptr[T], index: Int): T` — read element (no bounds check)
- `__ptr_write(ptr: Ptr[T], index: Int, val: T)` — write element (no bounds check)
- `__ptr_grow(ptr: Ptr[T], new_capacity: Int): Ptr[T]` — reallocate with larger capacity
- `__ptr_capacity(ptr: Ptr[T]): Int` — query allocated capacity

These are the irreducible operations. Everything else is built in KS.

### The aliasing problem

`Ptr[T]` is a handle (integer index). Copying a Ptr copies the handle, not the storage. Two Ptrs can alias the same allocation. If one deallocates, the other is dangling.

This is WHY Ptr operations are unsafe. Safe code can't create or manipulate Ptrs directly. Only `Buf[T]` (which manages the Ptr lifecycle) touches them, inside unsafe blocks.

When a `Buf[T]` is deep-copied (via DeepCopy protocol), it must allocate new storage and copy elements — not just copy the PtrId. This is the connection to the lifecycle-protocols proposal.

### What is `Buf[T]`?

A `kind` defined in `std/mem.ks`:

```ks
kind Buf[T] {
    ptr: Ptr[T],
    len: Int,
    cap: Int,
}

impl Buf {
    func new(): Buf[T] {
        unsafe {
            ret Buf[T] { ptr: __ptr_alloc(0), len: 0, cap: 0 }
        }
    }

    func with_capacity(cap: Int): Buf[T] {
        unsafe {
            ret Buf[T] { ptr: __ptr_alloc(cap), len: 0, cap: cap }
        }
    }

    func get(self, index: Int): T {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        unsafe { ret __ptr_read(self.ptr, index) }
    }

    func set(self, index: Int, val: T) {
        if index < 0 || index >= self.len {
            panic("index out of bounds")
        }
        unsafe { __ptr_write(self.ptr, index, val) }
    }

    func push(self, val: T) {
        if self.len == self.cap {
            self.grow()
        }
        unsafe { __ptr_write(self.ptr, self.len, val) }
        self.len = self.len + 1
    }

    func pop(self): Opt[T] {
        if self.len == 0 { ret Opt[T].None }
        self.len = self.len - 1
        unsafe { ret Opt[T].Some(__ptr_read(self.ptr, self.len)) }
    }

    func grow(self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 }
        unsafe {
            self.ptr = __ptr_grow(self.ptr, new_cap)
        }
        self.cap = new_cap
    }
}

impl Buf as Drop {
    func drop(self) {
        unsafe { __ptr_dealloc(self.ptr) }
    }
}

impl Buf as DeepCopy {
    func deep_copy(self): Buf[T] {
        let new_buf = Buf[T].with_capacity(self.cap)
        let i = 0
        while i < self.len {
            new_buf.push(self.get(i))
            i = i + 1
        }
        ret new_buf
    }
}
```

### What is `Arr[T]`?

A `kind` defined in `std/dsa.ks` (data structures and algorithms):

```ks
kind Arr[T] { buf: Buf[T] }

impl Arr {
    func new(): Arr[T] { ret Arr[T] { buf: Buf[T].new() } }
    func push(self, val: T) { self.buf.push(val) }
    func pop(self): Opt[T] { ret self.buf.pop() }
    func len(self): Int { ret self.buf.len }
    func get(self, index: Int): T { ret self.buf.get(index) }
    func set(self, index: Int, val: T) { self.buf.set(index, val) }
}

impl Arr as ToIter[T] {
    func to_iter(self): ArrIter[T] {
        ret ArrIter[T] { arr: self, idx: 0 }
    }
}
```

### `unsafe` blocks

`unsafe { ... }` is a block expression. Inside an unsafe block, intrinsic functions prefixed with `__` are available. Outside unsafe, calling `__ptr_alloc` etc. is a runtime error.

The unsafe block is the safety boundary. The invariant: if your KS code never writes `unsafe`, you can't cause memory corruption. All memory corruption is contained within unsafe blocks in trusted library code.

### Module namespacing

The layered architecture implies a module system. For now, Ptr and Buf can live in `std/prelude.ks` or a new `std/mem.ks` that's auto-loaded. Arr can be in `std/prelude.ks` or `std/dsa.ks`. The exact module loading mechanism is a separate concern (the `import` feature on the roadmap).

### Prerequisites

This proposal depends on:
- **Lifecycle protocols** (Drop, DeepCopy) — see [prop: lifecycle-protocols](lifecycle-protocols.md)
- **Generic methods** — `impl Buf[T] { ... }` requires generic impl targets (currently broken)
- **Method lookup fallback** — `Buf[Int]` methods need to resolve from `Buf` base type
- **`Self` type** — DeepCopy needs `func deep_copy(self): Self`
- **`unsafe` keyword** — lexer + parser + interpreter gating
- **`TypeExpr::Generic`** — Buf[T] field `ptr: Ptr[T]` requires TypeExpr to express generic type applications, not just bare params

### Open questions

- Should `Ptr[T]` be typed or untyped? Typed adds safety but complicates the prim. Untyped (`Ptr` not `Ptr[T]`) is simpler but pushes all type checking to Buf/Arr.
- Should `__ptr_read` return `T` or `Opt[T]`? Returning T is unsafe (might read uninitialized). Returning Opt adds ceremony.
- How does garbage collection interact? If a Buf forgets to call drop, the Ptr leaks. Should there be a GC fallback?
- Exact `unsafe` scoping rules: can unsafe blocks nest? Can a function be marked `unsafe func`?

## Decision
<!-- blank while open -->

## References
- [spec: type-system](../../docs/spec/type-system.md) — prim vs builtin layers
- [phil: stdlib](../../docs/phil/stdlib.md) — "List constructors need memory allocation (until self-hostable)"
- [prop: lifecycle-protocols](lifecycle-protocols.md) — Drop, Copy, DeepCopy
- Rust: `NonNull<T>` → `RawVec<T>` → `Vec<T>` layering
- Rust: `unsafe` blocks and the safety contract
- Python: `ctypes` for raw memory, `list` as the safe wrapper
