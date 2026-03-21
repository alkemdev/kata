# std.core — fundamental types
#
# Types the language itself depends on. Opt is used by iteration
# (Iter.next returns Opt[T]) and throughout the standard library.
# Res is used for error handling.

enum Opt[T] {
    Some(T),
    None,
}

enum Res[T, E] {
    Ok(T),
    Err(E),
}
