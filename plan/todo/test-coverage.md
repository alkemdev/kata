# TODO: Expand conformance test coverage

Areas with thin coverage:
- `arr/` — 4 tests for a heavily-used type. Add: negative index, index assignment, iteration edge cases, empty array operations
- `buf/` — 2 tests. Add: read/write at capacity boundary, grow behavior
- `ptr/` — 4 tests. Add: write-then-read roundtrip, multiple allocations
- `unsafe/` — 1 test. Add: nested unsafe, unsafe in method, unsafe required errors
- `impl/` — method dispatch edge cases: method on wrong type, Self type in various contexts
- `match/` — nested patterns (when implemented), guard expressions, all pattern types
- `index/` — 4 tests. Add: chained indexing (when nested assignment lands), index on custom types implementing GetItem

Also consider: a "stress test" directory with larger programs that exercise multiple features together.
