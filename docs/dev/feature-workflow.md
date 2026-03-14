# Feature development workflow

This document describes the standard process for adding a feature to KataScript. Each step maps to a natural commit point.

---

## Step 1 — Write the spec

Copy the template and fill it in:

```sh
cp docs/dev/feature-template.md docs/dev/specs/<feature>.md
```

Fill out:
- Summary (one sentence)
- Syntax delta (BNF)
- Semantics (eval behavior, type rules, error conditions)
- Examples (happy path + error cases with expected output)
- Interactions and non-goals
- Done criteria (add any feature-specific items)

Commit: `spec: add <feature>`

---

## Step 2 — Write failing conformance tests

Add fixture files in `tests/ks/<category>/`:

```
tests/ks/<category>/<name>.ks           input program
tests/ks/<category>/<name>.expected     expected stdout (happy path)
tests/ks/<category>/<name>.expected_err expected stderr fragment (error cases)
```

Rules:
- One behavior per test — don't combine happy path and error in one file.
- `.expected` implies exit 0; `.expected_err` implies nonzero exit.
- The fragment in `.expected_err` only needs to appear somewhere in stderr.

Register each test in `katars/tests/conformance.rs`:

```rust
mod <category> {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .join("tests/ks/<category>")
            .join(name)
    }

    #[test]
    fn <name>() { run_conformance_test(&fixture("<name>.ks")); }
}
```

Verify the tests fail with the binary in its current state:

```sh
cargo test --test conformance -- <category>::
```

Commit: `test: conformance for <feature>`

---

## Step 3 — Implement

Work in this order to minimize compiler friction:

### 3a — Lexer (`katars/src/ks/lexer.rs`)

1. Add `Token` variant (before `Ident` if it's a keyword).
2. Add `Display` arm.
3. Add lex unit test.

### 3b — AST (`katars/src/ks/ast.rs`)

1. Add new `Expr` or `Stmt` variant.
2. Ensure `#[derive(Serialize, Deserialize)]` is present.

### 3c — Parser (`katars/src/ks/parser.rs`)

1. Update BNF comment at top of file.
2. Add chumsky combinator for new production.
3. Add parse unit test that checks AST shape.

### 3d — Evaluator (`katars/src/ks/eval.rs`)

1. Add match arm in `eval_expr` or `exec_stmt`.
2. Thread `out: &mut impl Write` to any new helper that produces output.
3. Return `Err(String)` for all error conditions — no panics.

Commit: `feat: implement <feature>`

---

## Step 4 — Verify and close

1. Run full test suite: `cargo test`
2. Spot-check: `cargo run -- ks tests/ks/<category>/<name>.ks`
3. Check AST output: `cargo run -- ks --dump-ast tests/ks/<category>/<name>.ks | jq .`
4. Walk through done criteria in the spec; check each box.
5. Update spec status to `done`.

Commit: `spec: mark <feature> done`

---

## PR checklist

Before opening a PR, confirm:

- [ ] No `println!` in `eval.rs` (all output through the writer)
- [ ] BNF comment in `parser.rs` matches the implementation
- [ ] `cargo run -- ks --dump-ast <file> | jq .` works for new syntax
- [ ] `cargo test` green (unit + conformance)
- [ ] Spec file updated to `done`
