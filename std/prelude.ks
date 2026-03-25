# KataScript standard prelude
# Auto-loaded before user code. Re-exports from std modules.

# Scoped imports — makes std.core, std.mem, std.dsa browsable as module paths.
import std.core
import std.mem
import std.dsa

# Selective imports — pulls names into top-level scope for convenience.
import std.core.{Opt, Res, Iter, ToIter, Drop, Copy, Dupe, GetItem, SetItem}
import std.mem.{HeapAllocator, Ptr, Buf, heap}
import std.dsa.{Arr, ArrIter}
