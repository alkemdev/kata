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
plan/     design docs: philosophy, proposals, specs, roadmap
```

## Design

See [plan/](plan/) for the full plan.
