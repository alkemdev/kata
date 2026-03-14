# Kata

A small, self-hosting language ecosystem.

## Quick Start

```
cargo run -- ks <file.ks>
```

Example:

```
cargo run -- ks tests/hello.ks
# hello, world
```

## Structure

```
katars/   kata CLI + KataScript tree-walk evaluator
tests/    KataScript source files
docs/plan/  design docs: vision, architecture, roadmap
```

## Design

See [docs/plan/](docs/plan/) for the full plan.
