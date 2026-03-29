# KataScript standard prelude
# Auto-loaded before user code. Re-exports from std modules.

# Core — fundamental types and protocols (always in scope).
import core
import core.{Opt, Res, Iter, ToIter, Drop, Copy, Dupe, GetItem, SetItem, ToBin, Hash}

# Memory module — available for types that need allocation internals.
import mem

# Collections — Arr is the default collection.
import dsa.{Arr, ArrIter, Slot, Map, MapIter}
