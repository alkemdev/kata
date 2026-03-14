# Language design decisions

This directory records non-obvious design choices in KataScript: what alternatives were weighed, why one was chosen, and what was deferred. The Decision section of each closed doc is the source of truth for that choice.

## When to write one

Write a decision doc when:
- There are multiple non-obvious alternatives with real trade-offs
- The choice has downstream consequences (syntax, compatibility, eval behavior)
- It's likely to be revisited — either by you or by AI context
- You want a record of what was ruled out and why

Skip it when:
- There's one obvious option
- It's a pure implementation detail with no design surface
- The feature spec already captures the rationale fully

## How it relates to a feature spec

Decision = *why* the design choice was made.
Spec = *what* to build and *how*.

Typical flow: open decision → deliberate → decide → move to `done/` → write/update spec referencing decision.

Reference from a spec: `See [disc: func-vs-fn](../../disc/done/func-vs-fn.md)`

## Step-by-step process

### 1. Open

```sh
cp docs/disc/template.md docs/disc/open/<id>.md
```

Fill in Question, Context, and Alternatives. Leave Decision blank.

Commit: `decision: open <id>`

### 2. Deliberate

Write in the Discussion section incrementally as you think it through. No structure required — use it as a scratch pad.

Commit: `decision: discuss <id>`

### 3. Decide

Fill in the Decision section (Chosen, Rationale, Consequences). Then:

```sh
git mv docs/disc/open/<id>.md docs/disc/done/<id>.md
```

Update any specs that reference this decision. Update **Date done**.

Commit: `decision: close <id> — <chosen in a few words>`

## No re-editing rule

Once a file is in `done/`, the Decision section is immutable. The deliberation is part of the record.

To revisit a closed decision, create `<id>-2.md` in `open/` referencing the original. Do not edit the original.

## Index

### Open

*(none)*

### Done

- [func-vs-fn](done/func-vs-fn.md) — `func` vs `fn` as the function keyword
- [semicolons](done/semicolons.md) — required vs optional vs forbidden semicolons
- [ret-keyword](done/ret-keyword.md) — `return` vs `ret` for explicit return
