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

// Iteration protocol interfaces
type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}
