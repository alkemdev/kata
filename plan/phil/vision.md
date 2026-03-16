# Vision

Kata is a small, self-hosting language ecosystem built for clarity and control.

## What Kata Is

A language stack where every layer is legible and replaceable:

- **KataScript (ks)** — the surface language: clean syntax, dynamic typing, expression-oriented
- **Kata IR (kir)** — a simple, typed intermediate representation; the boundary between front-end and back-end
- **Kata VM (kvm)** — a bytecode VM that executes kir; the reference runtime
- **kata** — the CLI that ties it all together: `kata ks`, `kata run`, `kata kpm`, `kata kdb`

## Design Axioms

1. **Layers are independent.** ks → kir → kvm is a pipeline, not a monolith. Each layer has a defined input/output contract.
2. **Self-hosting is the goal.** The KataScript compiler should eventually be written in KataScript.
3. **Small core, grown stdlib.** The VM and IR are minimal. The standard library grows in KataScript.
4. **No magic.** Every behavior is traceable to a rule in the spec.

## Component Picture

```
source.ks
    │
    ▼
[ ks parser ] ──► AST
    │
    ▼
[ ks evaluator / compiler ]
    │
    ├──► tree-walk eval (phase 1–2)
    │
    └──► kir emission (phase 3+)
              │
              ▼
         [ kir optimizer ]
              │
              ▼
         [ kvm bytecode ]
              │
              ▼
         [ kvm runtime ]
```
