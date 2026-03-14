# Architecture

## Directory Layout (Target)

```
kata/
  Cargo.toml          # workspace root

  katars/             # kata CLI + ks evaluator (phase 1–2)
    Cargo.toml
    src/
      main.rs         # subcommand dispatch
      ks/
        mod.rs        # public interface: run(source) -> Result<()>
        lexer.rs      # tokenizer
        parser.rs     # AST construction
        ast.rs        # AST node types
        eval.rs       # tree-walk interpreter

  kir/                # IR definition + utilities (phase 3+)
    Cargo.toml
    src/
      lib.rs
      types.rs
      builder.rs

  kvm/                # bytecode VM (phase 3+)
    Cargo.toml
    src/
      lib.rs
      vm.rs
      chunk.rs
      opcode.rs

  std/                # KataScript standard library (phase 4+)
    core.ks
    io.ks
    collections.ks

  tests/              # integration / conformance tests
    ks/
      hello.ks
      ...

  docs/
    plan/             # this directory
    spec/             # language + IR spec (grows with phase 2)
```

## Crate Responsibilities

| Crate    | Phase | Role |
|----------|-------|------|
| `katars` | 1+    | CLI, ks tree-walk evaluator |
| `kir`    | 3+    | IR types, builder, pretty-printer |
| `kvm`    | 3+    | Bytecode VM, chunk format |

## Interface Contracts

### Phase 1–2: `ks::run(source: &str) -> Result<()>`

The evaluator owns lexing, parsing, and evaluation. Output goes to stdout.

### Phase 3+: `ks::compile(source: &str) -> Result<kir::Module>`

ks becomes a compiler front-end. kvm executes the module.
