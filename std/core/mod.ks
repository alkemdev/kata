# std.core — fundamental types and protocols
#
# Types and protocols the language itself depends on.
# The for-loop desugars to Iter/ToIter, scope exit dispatches Drop,
# and Opt/Res are used throughout the standard library.

# ── Fundamental types ────────────────────────────────────────────

enum Opt[T] {
    Val(T),
    Non,
}

impl Opt[T] {
    func unwrap(self): T {
        ret match self {
            Val(x) -> x,
            Non -> panic("Opt.unwrap called on Non"),
        }
    }

    func unwrap_or(self, default: T): T {
        ret match self {
            Val(x) -> x,
            Non -> default,
        }
    }
}

enum Res[T, E] {
    Val(T),
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
