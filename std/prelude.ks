# KataScript standard prelude
# Auto-loaded before user code.

# Core types (from std.core)
import std.core.{Opt, Res}

# Protocols — defined here until interfaces become exportable values.
# The for-loop desugars to Iter/ToIter, scope exit dispatches Drop.

type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}

type Drop {
    func drop(self)
}

type Copy {
    func copy(self): Self
}

type Dupe {
    func dupe(self): Self
}

# Standard library
import std.mem.{HeapAllocator, Ptr, Buf, heap}
import std.dsa.{Arr, ArrIter}
