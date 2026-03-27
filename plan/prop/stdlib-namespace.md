# Decision: Standard library namespace and module structure
**ID:** stdlib-namespace
**Status:** done
**Date opened:** 2026-03-27
**Date done:** 2026-03-27
**Affects:** stdlib | eval

## Question
Should the standard library require an `std.` prefix on imports, and how should modules be organized?

## Context
Every import currently requires `import std.X` (e.g., `import std.mem`, `import std.core`). Native intrinsics live under `std.mem.*`. The prefix adds ceremony without providing value — there's no third-party package ecosystem to disambiguate against. Additionally, the `dsa` module name ("data structures and algorithms") reads like a course title rather than a module name.

## Alternatives

### Option A: Keep `std.` prefix
**Pros:** Signals "ships with the language." Familiar from Rust/C++.
**Cons:** Pure overhead with no package ecosystem. Longer imports for no disambiguation benefit.

### Option B: Drop `std.` prefix, flat top-level modules
**Pros:** Shorter imports (`import mem`, `import collections`). Less ceremony. Matches Python (`import os`) and Go (`import "fmt"`). If a package system materializes later, `std` can be reserved then.
**Cons:** Loses the visual "this is stdlib" signal. Mild future cost if third-party modules collide.

### Rename `dsa`?
Considered `collections` (self-documenting, standard across Python/Java/Kotlin) but it's verbose. `dsa` is short, established, and reads fine to the author. Keep `dsa`.

## Discussion

### Module inventory after changes

| Module        | Contents                                   | In prelude? |
|---------------|--------------------------------------------|-------------|
| `core`        | Opt, Res, protocols (Iter, ToIter, Drop, Copy, Dupe, GetItem, SetItem, ToBin) | Yes — all names |
| `mem`         | Allocator, HeapAllocator, Ptr, Buf, heap, raw intrinsics | No — explicit import signals intent |
| `dsa`         | Arr, ArrIter, (future: Map, Set, Deque)    | Arr + ArrIter only |

Future modules (`io`, `fmt`, `math`, `net`) follow the same pattern: top-level name, cherry-picked into prelude if pervasive.

### Prelude design

The prelude remains a real file (`std/prelude.ks`), not compiler magic. It runs before user code and re-exports the "almost always needed" set:

```
import core
import core.{Opt, Res, Iter, ToIter, Drop, Copy, Dupe, GetItem, SetItem, ToBin}
import dsa.{Arr, ArrIter}
```

`mem` is deliberately excluded — touching raw memory deserves an explicit `import mem`.

Library code within std stays explicit about cross-module deps even when the prelude would cover them.

### On-disk layout

The directory remains `std/` (it *is* the standard library). The change is purely in the import namespace: the module resolver maps `import X` to `std/X/mod.ks` without requiring an `std.` prefix.

### Rust-side changes

1. Native function registry: `std.mem.*` → `mem.*`
2. Module resolver: map `import X` → `std/X/mod.ks` (drop `std.` prefix requirement)
3. Update `std/prelude.ks` and all cross-module imports within std

### `core` visibility

`core`'s names reach user code via the prelude's re-exports, not via special-casing in the resolver. A user *could* write `import core` explicitly, but never needs to. This keeps the mechanism transparent.

## Decision
**Chosen:** Option B — drop `std.` prefix, keep `dsa`
**Rationale:** No package ecosystem to namespace against; the prefix is pure ceremony. `dsa` is short and established.
**Consequences:**
- All imports change: `import std.X` → `import X`
- Native intrinsic paths change: `std.mem.*` → `mem.*`
- Module resolver maps `import X` → `std/X/mod.ks`
- Prelude updated, cross-module imports within std updated
- `mem` excluded from prelude (explicit import signals low-level intent)

## References
- Python: `import os`, `import collections`
- Go: `import "fmt"`, `import "net/http"`
- Rust: `std::` prefix, but has a prelude for core types
