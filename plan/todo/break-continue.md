# TODO: break and continue

## What

Add `break` and `continue` statements for `while` loops (and later `for` loops).

## Why

The `for` loop desugaring needs `break` to exit when the iterator returns `Opt.None`. Also useful independently — `while true { ... break }` is a common pattern.

## Design

Extend the `Flow` enum with two new variants:

```rust
pub enum Flow {
    Next(Value),
    Return(Value),
    Break,
    Continue,
}
```

- `break` and `continue` are statements (`Stmt::Break`, `Stmt::Continue`)
- Only valid inside loops — runtime error otherwise
- While loop catches `Flow::Break` (exits) and `Flow::Continue` (skips to next iteration)
- `Flow::Return` still propagates through both

## Scope

- Lexer: `break`, `continue` keywords
- AST: `Stmt::Break`, `Stmt::Continue`
- Parser: simple keyword statements
- Interpreter: extend `Flow`, handle in `Expr::While` (and later `Expr::For`)
- Tests: `tests/ks/while/` — break in while, continue in while, nested loops

## Depends on

Nothing — while loops already exist.

## Unlocks

- For loops (clean exit when iterator exhausted)
