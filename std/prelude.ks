// KataScript standard prelude
// Auto-loaded before user code.

enum Opt[T] {
    Some(T),
    None,
}

enum Res[T, E] {
    Ok(T),
    Err(E),
}
