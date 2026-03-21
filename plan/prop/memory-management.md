# Decision: memory management architecture
**ID:** memory-management
**Status:** open
**Date opened:** 2026-03-21
**Date done:** —
**Affects:** interpreter, syntax, stdlib, types

## Question
How should KataScript handle heap allocation, growable storage, and the safety boundary around raw memory operations?

## Design

### Layer stack

```
RawPtr          — prim Value, opaque handle to untyped storage
Ptr[T]          — kind, typed view over RawPtr, pointer arithmetic
Buf[T]          — kind, growable buffer (ptr + len + cap)
Arr[T]          — kind, safe user-facing array (iteration, Drop, Dupe)
```

### RawPtr — runtime primitive

`Value::RawPtr(u32)` — an opaque handle into the runtime's allocation table. Cannot be constructed from KS code (no `RawPtr { ... }` syntax). Only created by allocator intrinsics.

The runtime holds `allocations: Vec<Option<Vec<Value>>>`. A RawPtr's u32 indexes into this table. Read/write operations are element-granular (one Value per slot), not byte-granular.

RawPtr is the only new `Value` variant. Everything above it is KS `kind` types.

### Ptr[T] — typed pointer

```ks
kind Ptr[T] { raw: RawPtr }

impl Ptr[T] {
    func read(self, index: Int): T {
        unsafe { ret std.mem.read(self.raw, index) }
    }

    func write(self, index: Int, val: T) {
        unsafe { std.mem.write(self.raw, index, val) }
    }

    func offset(self, n: Int): Ptr[T] {
        # Returns a view at a different offset.
        # For now, just read/write with index arithmetic.
        # True pointer arithmetic deferred to compiler phase.
        ret self
    }
}
```

Ptr[T] adds type checking (val: T enforced at the method level). Not recommended for direct use — Buf and Arr wrap it.

### Allocator interface

```ks
type Allocator {
    func make(self, cap: Int): RawPtr
    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr
    func free(self, raw: RawPtr)
}
```

The runtime provides a default `HeapAllocator`:

```ks
kind HeapAllocator {}

impl HeapAllocator as Allocator {
    func make(self, cap: Int): RawPtr {
        unsafe { ret std.mem.alloc(cap) }
    }

    func grow(self, raw: RawPtr, old_cap: Int, new_cap: Int): RawPtr {
        unsafe {
            let new_raw = std.mem.alloc(new_cap)
            let i = 0
            while i < old_cap {
                std.mem.write(new_raw, i, std.mem.read(raw, i))
                i = i + 1
            }
            std.mem.free(raw)
            ret new_raw
        }
    }

    func free(self, raw: RawPtr) {
        unsafe { std.mem.free(raw) }
    }
}
```

Future allocators (BumpAllocator, ArenaAllocator, PoolAllocator) implement the same interface with different strategies.

### Buf[T] — allocated block

```ks
kind Buf[T] { ptr: Ptr[T], cap: Int }

impl Buf[T] {
    func read(self, index: Int): T { ret self.ptr.read(index) }
    func write(self, index: Int, val: T) { self.ptr.write(index, val) }
    func grow(self, new_cap: Int) { ... }  # realloc via allocator
}
```

Buf is purely about memory lifecycle — allocate, grow, free. No len, no bounds checking, no push/pop. It's a building block, not a user type.

### Arr[T] — user-facing array

```ks
kind Arr[T] { buf: Buf[T], len: Int }

impl Arr[T] {
    func push(self, val: T) { ... }
    func pop(self): Opt[T] { ... }
    func get(self, index: Int): T { ... }   # bounds-checked
    func set(self, index: Int, val: T) { ... }
    func len(self): Int { ret self.len }
}
impl Arr[T] as Drop { ... }           # auto-cleanup
impl Arr[T] as Dupe { ... }           # deep copy
impl Arr[T] as ToIter[T] { ... }      # iteration
```

Arr adds the "valid region" (len) on top of Buf's raw capacity. Bounds checking, push/pop, iteration — all at this layer.

### Intrinsics (std.mem)

Minimal runtime escape hatch, only callable in `unsafe` blocks:

| Intrinsic | Signature | Description |
|-----------|-----------|-------------|
| `std.mem.alloc` | `(cap: Int) -> RawPtr` | Allocate storage, return handle |
| `std.mem.free` | `(raw: RawPtr)` | Free storage |
| `std.mem.read` | `(raw: RawPtr, idx: Int) -> Value` | Read element |
| `std.mem.write` | `(raw: RawPtr, idx: Int, val)` | Write element |
| `std.mem.capacity` | `(raw: RawPtr) -> Int` | Query capacity |
| `std.mem.len` | `(raw: RawPtr) -> Int` | Query written length |

Note: `grow` is NOT an intrinsic — it's implemented in the allocator by alloc + copy + free. This keeps the intrinsic surface minimal.

### Changes from current implementation

| Current | New |
|---------|-----|
| `Ptr { _id: Int }` — forgeable struct | `Value::RawPtr(u32)` — opaque prim |
| `std.mem.grow` intrinsic | Removed — allocator handles it |
| `std.mem.dealloc` | Renamed to `std.mem.free` |
| `Buf[T] { ptr: Ptr, ... }` | `Buf[T] { ptr: Ptr[T], ... }` — typed |
| No allocator abstraction | `type Allocator` interface |

### Future: Byte and Char prims

`Byte` (u8) and `Char` (Unicode codepoint, u32) are planned as prim types. They enable byte-level I/O, string internals, and binary protocols. They're orthogonal to the allocation design — our "memory" is Value-granular. When the compiler arrives, RawPtr could optionally point to byte buffers for Byte-typed allocations.

### Prerequisites

- Generic methods (`impl Ptr[T] { ... }`) — ✅ done
- Method lookup fallback — ✅ done
- TypeExpr::Generic (field `ptr: Ptr[T]`) — ✅ done
- Drop protocol — ✅ done
- `unsafe` blocks — ✅ done

## Decision
<!-- blank while open — this is the proposed design, pending implementation -->

## References
- [spec: type-system](../../docs/spec/type-system.md)
- [phil: stdlib](../../docs/phil/stdlib.md)
- [prop: lifecycle-protocols](lifecycle-protocols.md)
- Rust: NonNull → RawVec → Vec
- Zig: std.mem.Allocator interface
