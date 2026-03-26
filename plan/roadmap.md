# Roadmap

## Phase 1 — Hello World (done)

**Goal:** `cargo run -- ks hello.ks` prints `hello, world`.

- [x] Workspace builds clean
- [x] `kata ks <file>` dispatches to ks evaluator
- [x] `print("hello, world")` evaluates and prints

---

## Phase 2 — Full KataScript Semantics (current)

**Goal:** KataScript is a real language with conformance tests.

- [x] Variables (`let`, ~~`const`~~) + assignment
- [x] Arithmetic, comparison, logical operators
- [x] `if`/`elif`/`else`
- [x] `while`
- [x] `enum` (sum types, generics, prelude Opt/Res)
- [x] `kind` (product types, generics, field access/mutation)
- [x] `impl` blocks + method dispatch
- [x] Mutable self (copy-in copy-out)
- [x] `bail`/`cont` (loop control flow)
- [x] `type` interfaces + `impl K as T` conformance
- [x] `for` loops + iteration protocol
- [x] Structured runtime errors (`ErrorKind` enum)
- [x] Functions (`func`, closures, first-class)
- [x] Strings with interpolation + escape sequences
- [x] Module system (`import`)
- [x] Memory stack (Ptr/Buf/Arr), arrays, array literals
- [x] Match expressions + `?` operator
- [x] Error handling (`Res[T,E]`, `?` propagate, `!` unwrap)
- [x] Indexing (`a[i]`, `a[i] = v`) via GetItem/SetItem
- [ ] Maps
- [x] String methods (native: len, char_len, contains, starts_with, ends_with, trim, to_upper, to_lower, replace, substr, split, to_int, to_float, to_bin)

---

## Phase 3 — kir + kvm Pipeline

**Goal:** `kata run` compiles ks → kir → kvm bytecode and executes. Results agree with `kata ks`.

Adds:
- `kir` crate: IR types, builder, pretty-printer
- `kvm` crate: bytecode VM
- ks compiler back-end that emits kir
- `kata run` subcommand
- Conformance: all phase 2 tests pass via both paths

---

## Phase 4 — `std/` in KataScript

**Goal:** Standard library written in KataScript, loaded by the runtime.

Adds:
- `std/core.ks`, `std/io.ks`, `std/collections.ks`
- Module loader in kvm
- Bootstrap: kvm can execute the stdlib before running user code

---

## Phase 5+ — Independent Extensions

These can proceed in any order after phase 3:

- **kpm** — package manager: manifest format, registry, dependency resolution
- **kdb** — debugger: breakpoints, step, inspect, REPL
- **Optimization** — kir passes: constant folding, dead code elimination, inlining
- **JIT** — native codegen via cranelift or LLVM
- **Self-hosting** — ks compiler written in KataScript
