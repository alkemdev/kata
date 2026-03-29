# core.res — Res[T, E], result type for recoverable errors

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
