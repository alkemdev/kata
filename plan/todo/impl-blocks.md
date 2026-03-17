# TODO: impl blocks + method dispatch

## What

Add `impl Kind { func method(self, ...) { ... } }` syntax and wire up `obj.method(args)` dispatch.

## Why

Foundation for iteration protocol (`.to_iter()`, `.next()`), operator overloading, and stdlib ergonomics (`.len()`, `.unwrap()`). Currently dot access is hardcoded for enums and namespaces — no way to define methods on user types.

## Design

Follow Option A from `plan/prop/method-dispatch.md` (Rust-style `impl` blocks).

- `impl` keyword (4 chars, fits the family: `func`/`kind`/`enum`/`type`/`impl`)
- `self` is explicit first param, read-only in this pass (mutable self is a separate todo)
- Method table lives in Interpreter (`HashMap<TypeId, HashMap<String, Value::Func>>`), not TypeRegistry — avoids Value↔TypeDef cycle
- `obj.method(args)` → eval_attr checks method table after existing enum/namespace cases
- Multiple `impl` blocks per type allowed

## Scope

- Lexer: `impl` keyword
- AST: `Stmt::Impl { type_name: String, methods: Vec<...> }`
- Parser: `impl` block parsing
- Interpreter: method table, dispatch in `eval_attr`, `self` binding
- Tests: `tests/ks/impl/` — Point with distance method, chained calls, multiple impl blocks

## Test sketch

```ks
kind Point { x: Int, y: Int }

impl Point {
    func sum(self): Int {
        ret self.x + self.y
    }
}

let p = Point { x: 3, y: 4 }
print(p.sum())
// → 7
```

## Depends on

Nothing — structs and dot access already exist.

## Unlocks

- Mutable self (next todo)
- Iteration protocol
- Operator overloading via abstract types
