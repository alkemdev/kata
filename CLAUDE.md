# kata — AI assistant guide

## What is kata?

kata is a personal programming language workbench: a KataScript interpreter (`kata ks`), a TUI REPL (`kata repl`), and supporting tooling. KataScript is a dynamically-typed, expression-oriented scripting language. The project exists to explore language design and compiler construction from first principles.

## Language status

- **Literals**: Int (BigInt), Float (f64), Str, Bool, Nil, Bin, Byte, Char, RawPtr
- **Strings**: double-quoted (`"..."`) with escape sequences + `{expr}` interpolation; single-quoted (`'...'`) with escapes only (no interpolation). Escapes: `\n`, `\t`, `\r`, `\0`, `\\`, `\'`, `\"`, `\xNN` (hex byte), `\uNNNN` (Unicode BMP), `\UNNNNNNNN` (full Unicode)
- **Byte strings**: `b"..."` (interpolation) and `b'...'` (literal) produce interned `Bin` values. `\xNN` produces a single raw byte. Display: `b'hello\xff\n'`
- **Variables**: let (binding), assignment (reassignment), lexical scoping, shadowing
- **Functions**: func, typed params, return type annotation, ret, closures
- **Operators**: +, -, *, /, eq, ne, lt, gt, le, ge, unary -, !, string concat — all via std.ops
- **Types**: enum (generics), struct (kind keyword, generics, field access/assignment), types as values, typeof, Opt[T]/Res[T,E] in std.core
- **Methods**: impl blocks with `@` binding sigil (`impl Foo[@T]` generic, `impl Foo[Int]` specialized), method dispatch with base-type fallback, mutable self (copy-in copy-out), `self`/`Self` keywords, static methods (no self)
- **Interfaces**: type (abstract interface), impl K as T (conformance), Iter[T]/ToIter[T]/Drop/Copy/Dupe/ToBin in std.core
- **Control flow**: if/elif/else (expression), while, for (iterator protocol), bail, cont, && || (short-circuit)
- **Blocks**: with (scoped bindings), unsafe (gates std.mem intrinsics)
- **Memory**: RawPtr (opaque prim), Ptr[T], Buf[T], Arr[T] — layered stack with Allocator interface. Bin interning (Arc<[u8]>, pointer-equality fast path)
- **Modules**: import std.mem (scoped), import std.mem.{Ptr, Buf} (selective). Hierarchical std: std.core, std.mem, std.dsa
- **Lifecycle**: Drop protocol (auto-called on scope exit), Self type in impl blocks
- **Error handling**: Res[T,E] + postfix `?` (unwrap or propagate) + postfix `!` (unwrap or panic), Res methods (unwrap, unwrap_or, is_val, is_err)
- **Pattern matching**: match expression with variant/literal/wildcard/binding patterns
- **Collections**: array literals `[1, 2, 3]`, `a[i]`/`a[i] = v` indexing via GetItem/SetItem protocols
- **String methods**: len, char_len, contains, starts_with, ends_with, trim, trim_start, trim_end, to_upper, to_lower, replace, substr, split, to_int, to_float, to_bin — all native
- **Not yet**: maps, const

## Project layout

- `katars/src/ks/` — interpreter source: lexer, AST, parser, type registry, values, native functions, interpreter
- `katars/src/tui/` — ratatui REPL
- `std/` — KataScript standard library: `prelude.ks` (auto-loaded re-exports), `core/` (Opt, Res, protocols), `mem/` (Ptr, Buf, Allocator), `dsa/` (Arr)
- `tests/ks/<feature>/` — conformance fixtures, one `.ks` + `.expected` pair per behavior
- `demos/` — example KataScript programs
- `docs/` — permanent reference: `phil/` (philosophy), `spec/` (approved decisions)
- `plan/` — work tracking: `prop/` (active proposals), `todo/`, `work/` (in progress), `roadmap.md`

## Key invariants

- **`Token` enum is the source of truth** for all lexable syntax. The lexer, parser, and any tooling derive from it.
- **`Interpreter` owns type registry + scope + all eval logic** — it is the single entry point for execution.
- **All output goes through `&mut impl Write`** — never use `println!` inside the interpreter.
- **Types are first-class values** — `print(Int)` works; types flow through the same `Value` enum as data.
- **Real type checking** — enum/struct construction, typed function params, and returns are validated at runtime via `TypeId`.
- **`TypeId` handles, not strings** — type identity is a registry index, not a name comparison.
- **No panics in the interpreter** — return `Err(RuntimeError)` with a structured `ErrorKind` for all runtime errors. `ErrorKind::Other(String)` exists as a migration bridge but should not be used in new code.
- **Serde on all AST types** — `Expr`, `Stmt`, `Program` must derive `Serialize`/`Deserialize` so `--dump-ast | jq .` works.
- **BNF comment in `parser.rs` stays current** — update it before writing parser code.
- **One behavior per conformance test** — each `.ks` + `.expected` pair tests exactly one thing.
- **Rich data models over string hacks** — use the Rust type system to model domain concepts. Don't use `String` when a structured type (enum, newtype, AST node) would enforce correctness at compile time. One representation per concept.
- **All type annotations in the AST are `Spanned<Expr>`** — never revert to string-based type annotations.

## Design decisions

Before proposing a change to KataScript syntax or semantics, check `docs/spec/` — the Decision section is the source of truth for that choice. `plan/prop/` lists choices still being weighed.

Before implementing a feature with non-obvious design alternatives, check whether a proposal in `plan/prop/` already covers it. If not, suggest creating one.

Reference (`docs/`):
- **`docs/phil/`** — guiding philosophy. Rarely changes. "Why are we building it this way?"
- **`docs/spec/`** — approved decisions. Immutable. "Why did we choose Y?" To revisit, open a new proposal.

Work tracking (`plan/`):
- **`plan/prop/`** — active proposals. Where design alternatives are weighed. When decided, produce a `docs/spec/` entry and delete the proposal.
- **`plan/todo/`** — concrete action items. No design alternatives to weigh, just work to do.
- **`plan/work/`** — actively in progress. Move here from `todo/` when starting; delete when done (the commit + spec capture the result).

## Type system reference

See [spec: type-system](docs/spec/type-system.md) for the canonical type design. Three keywords for defining types: `kind` (concrete product type), `enum` (concrete sum type), `type` (abstract interface). Conformance declared via `impl Kind as Type { ... }`. Two-layer architecture: prim types (runtime-handled) and builtin types (self-hostable in KS). See `docs/phil/stdlib.md` for the division. Type names are PascalCase; they remain Ident tokens in the lexer.

## Running tests

```sh
cargo test                                    # all unit tests + conformance
cargo test --test conformance                 # just the conformance runner
cargo test --test conformance -- func/        # filter by feature
cargo run -- ks tests/ks/func/basic.ks        # run a specific script
cargo run -- repl                             # TUI REPL
```

## How to add a feature

Three tiers depending on design complexity:

**Tier 1 — obvious implementation.** Check `plan/prop/` for relevant proposals. Write conformance tests (`.ks` + `.expected` in `tests/ks/<feature>/`). Implement in order: lexer → AST → parser → interpreter. Update BNF in `parser.rs`. After implementation, check whether new code duplicates existing patterns — if so, extract shared machinery before moving on. Commit as `feat: <feature>`.

**Tier 2 — design fork.** Open a proposal in `plan/prop/` using the template. Deliberate on alternatives. Close by moving to `docs/spec/`. Then implement as Tier 1.

**Tier 3 — structural change.** Proposal if needed. Multi-commit along natural boundaries. Each commit should be independently correct.

## Commit conventions

- `feat:` / `refactor:` / `fix:` / `docs:` / `infra:`
- Tests land with the feature, one commit per feature
- Message explains the *why* and the *unexpected*, not just what changed

## Keeping docs current

After a feature commits:
- Update the **Language status** section above (move items off "Not yet" as they land)
- Close relevant `plan/prop/` proposals — fill in the Decision section and move to `docs/spec/`
- Update `plan/roadmap.md` checkboxes
- Delete completed `plan/todo/` items (the commit + spec capture the result)

**Conformance runner contract:** the runner matches `*.ks` to sibling `*.expected` (stdout) or `*.expected_err` (stderr substring) by name, runs the script via `cargo run -- ks`, and diffs output. One behavior per test file.

## Extending the lexer (`lexer.rs`)

- Add the new `Token` variant before `Ident` if it's a keyword (logos matches in declaration order).
- Multi-char operators must appear before any single-char prefix that could match first.
- Add a `Display` impl arm so the token prints readably in error messages.
- Add a lex unit test.

## Extending the parser (`parser.rs`)

- Update the BNF comment at the top of the file first.
- Use chumsky combinators (`just`, `choice`, `recursive`, etc.). See existing productions for style.
- Add a parse unit test that checks the AST shape, not just "no error".

## Architecture

The interpreter is split across three files in `katars/src/ks/`:

- **`types.rs`** — `TypeRegistry` manages `TypeDef`s keyed by `TypeId`. All type identity is handle-based.
- **`value.rs`** — `Value` enum: the runtime representation of all KataScript values.
- **`error.rs`** — `ErrorKind` enum: structured runtime errors with raw data (TypeIds, names, counts). Formatting deferred to `format_with(&TypeRegistry)` at render time.
- **`interpreter.rs`** — `Interpreter` struct owns the `TypeRegistry` and a stack of lexical scope frames. All `exec_stmt`/`eval_expr` logic lives here.

Key patterns to understand by reading the code:
- **Postfix chains** — member access, indexing, and calls compose uniformly via `Expr::Attr`, `Expr::Item`, `Expr::Call`.
- **Operator dispatch** — operators go through `std.ops` (a module in the native function tree). `&&`/`||` are control flow, not operators — they short-circuit.
- **Truthiness** — nil, false, 0, 0.0, "" are falsy; everything else is truthy.
- **Builtins** — native functions dispatched via `NativeFnRegistry` + handler function pointers. No string matching at call time.
