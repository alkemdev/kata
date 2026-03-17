# TODO: for loops + iteration protocol

## What

Add `for x in expr { body }` loops. The expression must be iterable — it provides `.to_iter()` returning an iterator with `.next() -> Opt[T]`.

## Why

Phase 2 roadmap item. Unlocks general iteration over any user-defined type.

## Design

Protocol (two methods, eventually formalized as abstract `type`s):
- **ToIter**: collection has `.to_iter()` → returns an iterator
- **Iter**: iterator has `.next()` → returns `Opt[T]` (`Some(value)` or `None` to stop)

For-loop desugaring:
```
for x in collection { body }
→
let __iter = collection.to_iter()
while true {
    let __next = __iter.next()
    if __next eq Opt.None { break }
    let x = unwrap(__next)
    body
}
```

The interpreter implements this desugaring internally — no actual while loop emitted, but the same Flow handling.

## Scope

- Lexer: `for`, `in` keywords
- AST: `Expr::For { binding, iter_expr, body }`
- Parser: `for_expr` production
- Interpreter: eval For by calling `.to_iter()`, looping `.next()`, extracting Opt.Some values
- Tests: `tests/ks/iter/` — IntRange defined as a user type with to_iter/next

## Test sketch

```ks
kind IntRange { start: Int, end: Int }
kind IntRangeIter { current: Int, end: Int }

impl IntRange as ToIter[Int] {
    func to_iter(self): IntRangeIter {
        ret IntRangeIter { current: self.start, end: self.end }
    }
}

impl IntRangeIter as Iter[Int] {
    func next(self): Opt[Int] {
        if self.current < self.end {
            let val = self.current
            self.current = self.current + 1
            ret Opt[Int].Some(val)
        }
        ret Opt[Int].None
    }
}

for x in IntRange { start: 0, end: 3 } {
    print(x)
}
// → 0, 1, 2
```

## Depends on

- impl blocks + method dispatch
- Mutable self (iterator `.next()` advances state)
- break/continue (for-loop exit on `Opt.None`)

## Unlocks

- List/Map iteration (when collections land)
- `type Iter[T]` / `type ToIter[T]` formalization
- Comprehensions, generators (future)
