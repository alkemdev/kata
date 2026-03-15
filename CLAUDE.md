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
│   │   │   ├── mod.rs          public API: lex(), parse(), run()
│   │   │   ├── lexer.rs        logos-based lexer; Token enum is source of truth
│   │   │   ├── ast.rs          AST types (Expr, Stmt, Program); all serde-annotated
│   │   │   ├── parser.rs       chumsky parser; BNF grammar comment kept current
│   │   │   └── eval.rs         tree-walk evaluator; exec_program takes &mut impl Write
│   │   └── tui/
│   │       └── mod.rs          ratatui REPL; captures eval output into history pane
│   └── tests/
│       └── conformance.rs      subprocess-based conformance runner
├── std/                        KataScript standard library (written in KS)
│   └── prelude.ks              auto-loaded: Opt, Res, core utilities
├── tests/
│   └── ks/
│       └── syntax/             conformance fixtures
│           ├── expr/
│           ├── error/
│           ├── func/
│           ├── if/
│           ├── for/
│           ├── literal/
│           ├── stmt/
│           ├── type/
│           ├── warning/
│           └── while/
│               ├── <name>.ks
│               ├── <name>.expected     (exit 0, stdout match)
│               └── <name>.expected_err (nonzero exit, stderr contains fragment)
└── docs/
    ├── plan/                   vision, architecture, roadmap
    ├── dev/                    feature specs and workflow
    │   ├── feature-template.md spec template
    │   ├── feature-workflow.md step-by-step process
    │   └── specs/              per-feature specs (one file each)
    └── disc/                   language design decisions
        ├── README.md           workflow: when to write one, step-by-step process
        ├── template.md         copy-paste template
        ├── open/               decisions still being weighed
        └── done/               closed decisions — source of truth, don't edit
```

## Key invariants

- **`Token` enum is the source of truth** for all lexable syntax. The lexer, parser, and any tooling derive from it.
- **`exec_program` takes `&mut impl Write`** — never use `println!` inside the evaluator. All output goes through the writer.
- **One behavior per conformance test** — each `.ks` + `.expected` pair tests exactly one thing.
- **BNF comment in `parser.rs` stays current** — update it before writing parser code.
- **Serde on all AST types** — `Expr`, `Stmt`, `Program` must derive `Serialize`/`Deserialize` so `--dump-ast | jq .` works.
- **No panics in `eval.rs`** — return `Err(String)` for all runtime errors.

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
cargo test --test conformance -- print::      # filter by category
cargo run -- ks tests/ks/print/hello.ks      # run a specific script
cargo run -- repl                             # TUI REPL
```

## How to add a feature

Full process in `docs/dev/feature-workflow.md`. Short version:

1. **Spec** — copy `docs/dev/feature-template.md` to `docs/dev/specs/<feature>.md`, fill it out.
2. **Conformance tests** — add `.ks` + `.expected` (or `.expected_err`) fixtures in `tests/ks/<category>/`, register them in `katars/tests/conformance.rs`.
3. **Implement** in order: lexer → AST → parser → eval.
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

## Extending the evaluator (`eval.rs`)

- All functions that produce output take `out: &mut impl Write` and thread it down.
- Return `Err(String)` for runtime errors — no `panic!`, no `unwrap` on user data.
- New builtins go in the `call` match arm; new statement types go in `exec_stmt`.
