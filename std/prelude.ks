# KataScript standard prelude
# Auto-loaded before user code.

# ── Core types ───────────────────────────────────────────────────

enum Opt[T] {
    Some(T),
    None,
}

enum Res[T, E] {
    Ok(T),
    Err(E),
}

# ── Iteration protocol ───────────────────────────────────────────

type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}

# ── Lifecycle protocols ──────────────────────────────────────────

type Drop {
    func drop(self)
}

type Copy {
    func copy(self): Self
}

type Dupe {
    func dupe(self): Self
}

# ── Standard library modules ─────────────────────────────────────

import std.dsa
