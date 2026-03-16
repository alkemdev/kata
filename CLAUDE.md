# kata — AI assistant guide

## What is kata?

kata is a personal programming language workbench: a KataScript interpreter (`kata ks`), a TUI REPL (`kata repl`), and supporting tooling. KataScript is a dynamically-typed, expression-oriented scripting language. The project exists to explore language design and compiler construction from first principles.

## Directory map

```
kata/
├── Cargo.toml                  workspace manifest
├── CLAUDE.md                   this file
├── README.md
├── katars/                     the kata Rust crate (binary: `kata`)
│   ├── src/
│   │   ├── main.rs             clap CLI entrypoint — subcommands: ks, repl
│   │   ├── ks/
│   │   │   ├── mod.rs          public API: Interpreter, lex(), parse(), run()
│   │   │   ├── lexer.rs        logos-based lexer; Token enum is source of truth
│   │   │   ├── ast.rs          AST types (Expr, Stmt, Param, Program)
│   │   │   ├── parser.rs       chumsky parser; postfix chain (Attr/Item/Call)
│   │   │   ├── types.rs        TypeId, TypeDef, TypeRegistry — real type system
│   │   │   ├── value.rs        Value enum (Int, Float, Str, Func, Enum, Type, ...)
│   │   │   └── interpreter.rs  Interpreter struct — owns types, scope, eval logic
│   │   └── tui/
│   │       └── mod.rs          ratatui REPL
│   └── tests/
│       └── conformance.rs      subprocess-based conformance runner (auto-discovery)
├── std/                        KataScript standard library (written in KS)
│   └── prelude.ks              auto-loaded: Opt[T], Res[T, E]
├── tests/ks/                   conformance fixtures by feature
│   ├── int/, float/, str/, bool/, nil/, bin/   literal tests
│   ├── let/                    variable binding + scoping
│   ├── func/                   functions, typed params, ret
│   ├── with/                   scoped blocks
│   ├── enum/                   enum types, generics, prelude
│   ├── type/                   typeof, types as values
│   ├── call/                   general call expressions
│   ├── ops/                    operators, std.ops dispatch
│   └── parse/                  parser error recovery
└── docs/
    ├── plan/                   vision, architecture, roadmap, stdlib
    ├── dev/                    feature specs and workflow
    └── disc/                   language design decisions (open/ and done/)
```

## Key invariants

- **`Token` enum is the source of truth** for all lexable syntax. The lexer, parser, and any tooling derive from it.
- **`Interpreter` owns type registry + scope + all eval logic** — it is the single entry point for execution.
- **All output goes through `&mut impl Write`** — never use `println!` inside the interpreter.
- **Types are first-class values** — `print(Int)` works; types flow through the same `Value` enum as data.
- **Real type checking** — enum construction, typed function params, and returns are validated at runtime via `TypeId`.
- **`TypeId` handles, not strings** — type identity is a registry index, not a name comparison.
- **No panics in the interpreter** — return `Err(String)` for all runtime errors.
- **Serde on all AST types** — `Expr`, `Stmt`, `Program` must derive `Serialize`/`Deserialize` so `--dump-ast | jq .` works.
- **BNF comment in `parser.rs` stays current** — update it before writing parser code.
- **One behavior per conformance test** — each `.ks` + `.expected` pair tests exactly one thing.

## Language design decisions

Before proposing a change to KataScript syntax or semantics, check `docs/disc/done/` — the Decision section is the source of truth for that choice. `docs/disc/open/` lists choices still being weighed.

Before implementing a feature with non-obvious design alternatives, check whether an open decision doc already covers it. If not, suggest creating one.

## Type system reference

See [disc: type-system](docs/disc/open/type-system.md) for the canonical type design.

Two-layer architecture: prim types (runtime-handled) and builtin types (self-hostable).
Prim: Int, I8–I256, U8–U256, F16–F128, Float (deferred), Str, Bin, Nil, Bool, Func.
Builtin: List, Map, Set, Range, Opt, Res — all defined in KS itself (see `docs/plan/stdlib.md`).
Type names are PascalCase; they remain Ident tokens in the lexer.

## Running tests

```sh
cargo test                                    # all unit tests + conformance
cargo test --test conformance                 # just the conformance runner
cargo test --test conformance -- func/        # filter by feature
cargo run -- ks tests/ks/func/basic.ks        # run a specific script
cargo run -- repl                             # TUI REPL
```

## How to add a feature

Full process in `docs/dev/feature-workflow.md`. Short version:

1. **Spec** — copy `docs/dev/feature-template.md` to `docs/dev/specs/<feature>.md`, fill it out.
2. **Conformance tests** — add `.ks` + `.expected` (or `.expected_err`) fixtures in `tests/ks/<feature>/`. Auto-discovered by the conformance runner.
3. **Implement** in order: lexer → AST → parser → interpreter.
4. **Verify** done criteria; mark spec as done.

Natural commit points: `spec: add <feature>`, `test: conformance for <feature>`, `feat: implement <feature>`, `spec: mark <feature> done`.

## Extending the lexer (`lexer.rs`)

- Add the new `Token` variant before `Ident` if it's a keyword (logos matches in declaration order).
- Multi-char operators must appear before any single-char prefix that could match first.
- Add a `Display` impl arm so the token prints readably in error messages.
- Add a lex unit test.

## Extending the parser (`parser.rs`)

- Update the BNF comment at the top of the file first.
- Use chumsky combinators (`just`, `choice`, `recursive`, etc.). See existing productions for style.
- Add a parse unit test that checks the AST shape, not just "no error".

## Interpreter architecture

The interpreter is split across three files:

- **`types.rs`** — `TypeRegistry` manages `TypeDef`s keyed by `TypeId`. All type identity is handle-based. Enum variant definitions, generic parameters, and type expressions live here.
- **`value.rs`** — `Value` enum: the runtime representation of all KataScript values. Includes `Int`, `Float`, `Str`, `Bool`, `Nil`, `Func`, `Enum` (constructed variant), `Type` (reified type-as-value), etc.
- **`interpreter.rs`** — `Interpreter` struct owns the `TypeRegistry` and a stack of lexical scope frames. All statement execution (`exec_stmt`) and expression evaluation (`eval_expr`) live here.

## Expression model

The parser produces a postfix chain for member access, indexing, and calls:

- `Expr::Attr` — dot access: `foo.bar`
- `Expr::Item` — bracket access: `foo[0]`
- `Expr::Call` — function/method call: `foo(x)`, `Opt.Some(1)`

These compose uniformly — `a.b[c](d)` is a chain, not special-cased syntax.

## Operator dispatch (`std.ops`)

Operators dispatch through a unified system:
- `a + b` evaluates to `Expr::BinOp { op: Add, ... }` and calls `eval_binop`
- `std.ops.add(a, b)` is a `BuiltinFn` that calls the same `eval_binop`
- `&&` and `||` are **not** operators — they're `Expr::And`/`Expr::Or` control flow nodes with short-circuit evaluation

The `std` namespace is a `Value::Namespace`. Attribute access chains: `std` → `std.ops` (sub-namespace) → `std.ops.add` (`BuiltinFn`). Known sub-namespaces are listed in `eval_attr`.

Truthiness (`std.ops.truth`): nil, false, 0, 0.0, "" are falsy; everything else is truthy.

See [disc: operator-overloading](docs/disc/open/operator-overloading.md) for the plan to support user-defined operator dispatch.

## Adding a builtin function

Add a match arm in `Interpreter::call_builtin` in `interpreter.rs`. Builtins receive evaluated `&[Value]` args and return `Option<Result<Value, String>>` — return `None` to fall through to user-defined function lookup.

## Adding a new type

1. Add a `TypeDef` variant or register it in `TypeRegistry::with_prims()` in `types.rs`.
2. Add a corresponding `Value` variant in `value.rs` if it needs a distinct runtime representation.
3. Handle construction and operations in the relevant `Interpreter` methods.
