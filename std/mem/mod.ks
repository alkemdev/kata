# mem — Memory management primitives
#
# Raw memory intrinsics (mem namespace, require unsafe):
#   mem.alloc(cap)              -> RawPtr
#   mem.free(raw: RawPtr)
#   mem.read(raw: RawPtr, idx)  -> Value
#   mem.write(raw: RawPtr, idx, val)
#   mem.capacity(raw: RawPtr)   -> Int
#   mem.len(raw: RawPtr)        -> Int
#
# Re-exports from sub-modules.

import mem.allocator.{Allocator, HeapAllocator, heap}
import mem.ptr.{Ptr}
import mem.buf.{Buf}
