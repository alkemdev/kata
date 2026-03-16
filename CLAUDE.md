# kata — AI assistant guide

## What is kata?

kata is a personal programming language workbench: a KataScript interpreter (`kata ks`), a TUI REPL (`kata repl`), and supporting tooling. KataScript is a dynamically-typed, expression-oriented scripting language. The project exists to explore language design and compiler construction from first principles.

## Language status

- **Literals**: Int (BigInt), Float (f64), Str, Bool, Nil, Bin
- **Variables**: let (binding), assignment (reassignment), lexical scoping, shadowing
- **Functions**: func, typed params, return type annotation, ret, closures
- **Control flow**: if/elif/else (expression), while, && || (short-circuit)
- **Operators**: +, -, *, /, eq, ne, lt, gt, le, ge, unary -, !, string concat — all via std.ops
- **Types**: enum (generics), types as values, typeof, Opt[T]/Res[T,E] in prelude
- **Blocks**: with (scoped bindings)
- **Not yet**: for, lists, maps, string interpolation, const, error handling, modules, break/continue

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
│   ├── if/                     if/elif/else expressions
│   ├── while/                  while loops
│   └── parse/                  parser error recovery
└── plan/
    ├── phil/                   guiding philosophy — why, not what
    │   ├── vision.md           design axioms, 5-phase bootstrap
    │   └── stdlib.md           runtime intrinsics vs KS-defined types
    ├── prop/                   active proposals — where discussion happens
    │   ├── template.md         proposal template
    │   ├── type-system.md
    │   ├── nil-option.md
    │   ├── error-handling.md
    │   └── operator-overloading.md
    ├── spec/                   approved decisions — immutable
    │   ├── func-vs-fn.md
    │   ├── semicolons.md
    │   ├── ret-keyword.md
    │   └── block-syntax.md
    └── roadmap.md              phased milestones
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

## Design decisions

Before proposing a change to KataScript syntax or semantics, check `plan/spec/` — the Decision section is the source of truth for that choice. `plan/prop/` lists choices still being weighed.

Before implementing a feature with non-obvious design alternatives, check whether a proposal in `plan/prop/` already covers it. If not, suggest creating one.

Three categories:
- **`plan/phil/`** — guiding philosophy. Rarely changes. "Why are we building it this way?"
- **`plan/prop/`** — active proposals. Where design alternatives are weighed. Closed by moving to `spec/`. "What should we do about X?"
- **`plan/spec/`** — approved decisions. Immutable. "Why did we choose Y?" To revisit, open a new proposal.

## Type system reference

See [prop: type-system](plan/prop/type-system.md) for the canonical type design.

Two-layer architecture: prim types (runtime-handled) and builtin types (self-hostable).
Prim: Int, I8–I256, U8–U256, F16–F128, Float (deferred), Str, Bin, Nil, Bool, Func.
Builtin: List, Map, Set, Range, Opt, Res — all defined in KS itself (see `plan/phil/stdlib.md`).
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

Three tiers depending on design complexity:

**Tier 1 — obvious implementation.** Check `plan/prop/` for relevant proposals. Write conformance tests (`.ks` + `.expected` in `tests/ks/<feature>/`). Implement in order: lexer → AST → parser → interpreter. Update BNF in `parser.rs`. Commit as `feat: <feature>`.

**Tier 2 — design fork.** Open a proposal in `plan/prop/` using the template. Deliberate on alternatives. Close by moving to `plan/spec/`. Then implement as Tier 1.

**Tier 3 — structural change.** Proposal if needed. Multi-commit along natural boundaries. Each commit should be independently correct.

## Commit conventions

- `feat:` / `refactor:` / `fix:` / `docs:` / `infra:`
- Tests land with the feature, one commit per feature
- Message explains the *why* and the *unexpected*, not just what changed

## Keeping docs current

After a feature commits, update the **Language status** section above.

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

See [prop: operator-overloading](plan/prop/operator-overloading.md) for the plan to support user-defined operator dispatch.

## Adding a builtin function

Add a match arm in `Interpreter::call_builtin` in `interpreter.rs`. Builtins receive evaluated `&[Value]` args and return `Option<Result<Value, String>>` — return `None` to fall through to user-defined function lookup.

## Adding a new type

1. Add a `TypeDef` variant or register it in `TypeRegistry::with_prims()` in `types.rs`.
2. Add a corresponding `Value` variant in `value.rs` if it needs a distinct runtime representation.
3. Handle construction and operations in the relevant `Interpreter` methods.
