# core — fundamental types and protocols
#
# Types and protocols the language itself depends on.
# The for-loop desugars to Iter/ToIter, scope exit dispatches Drop,
# and Opt/Res are used throughout the standard library.

# ── Fundamental types ────────────────────────────────────────────

enum Opt[T] {
    Val(T),
    Non,
}

impl Opt[@T] {
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

impl Res[@T, @E] {
    func unwrap(self): T {
        ret match self {
            Val(x) -> x,
            Err(e) -> panic("Res.unwrap called on Err"),
        }
    }

    func unwrap_or(self, default: T): T {
        ret match self {
            Val(x) -> x,
            Err(e) -> default,
        }
    }

    func unwrap_err(self): E {
        ret match self {
            Val(x) -> panic("Res.unwrap_err called on Val"),
            Err(e) -> e,
        }
    }

    func is_val(self): Bool {
        ret match self {
            Val(x) -> true,
            Err(e) -> false,
        }
    }

    func is_err(self): Bool {
        ret match self {
            Val(x) -> false,
            Err(e) -> true,
        }
    }
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

# ── Indexing protocols ─────────────────────────────────────────

type GetItem[K, V] {
    func get_item(self, key: K): V
}

type SetItem[K, V] {
    func set_item(self, key: K, val: V)
}

# ── Conversion protocols ─────────────────────────────────────

type ToBin {
    func to_bin(self): Bin
}
