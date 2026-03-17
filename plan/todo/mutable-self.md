# TODO: mutable self in methods

## What

Allow methods to mutate `self.field`, with changes propagated back to the caller's variable.

## Why

The iteration protocol requires `.next()` to advance iterator state (`self.current += 1`). Without mutable self, methods are pure observers — useful but not sufficient.

## Design

Copy-in copy-out semantics (Swift's `mutating` approach):

1. `obj.method(args)` — copy `obj` into `self`
2. Method body runs, can do `self.field = value` (uses existing `exec_attr_assign`)
3. After method returns, write the (possibly mutated) `self` back to the caller's variable

Requires the call site to be `variable.method()` (the receiver is `Expr::Name`). For chained access (`a.b.method()`), the interpreter walks back to the root variable — same pattern `exec_attr_assign` already uses.

Open question: should ALL methods be mutating, or should there be an opt-in marker? Starting with all-mutating is simpler. A `mut` qualifier can come later if needed.

## Scope

- Interpreter: after method body returns, write `self` back to caller's scope
- Tests: `tests/ks/impl/` — Counter with `.increment()`, verify mutation persists

## Test sketch

```ks
kind Counter { value: Int }

impl Counter {
    func increment(self) {
        self.value = self.value + 1
    }

    func get(self): Int {
        ret self.value
    }
}

let c = Counter { value: 0 }
c.increment()
c.increment()
print(c.get())
// → 2
```

## Depends on

- impl blocks + method dispatch

## Unlocks

- Iteration protocol (`.next()` mutates iterator state)
