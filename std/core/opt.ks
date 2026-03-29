# core.opt — Opt[T], optional value

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
