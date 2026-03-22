# TODO: Migrate remaining ErrorKind::Other usage

`ErrorKind::Other(String)` is a migration bridge for unstructured error messages. New code should use structured variants. Audit and replace all remaining uses.

Known sites:
- `panic()` native function produces `ErrorKind::Other(msg)` — should this get its own variant like `ErrorKind::Panic { message: String }`?
- `Expr::Bang` unwrap-on-error produces `ErrorKind::Other(format!("unwrap on ..."))` — should be a structured variant like `ErrorKind::UnwrapFailed { type_name, variant_name }`
- `call_method` abnormal flow: `ErrorKind::Other(format!("method '{}' returned abnormal flow"))` — edge case, may be unreachable

Grep: `ErrorKind::Other` in `interpreter.rs` and `native.rs`
