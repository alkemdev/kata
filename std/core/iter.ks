# core.iter — Iter[T] and ToIter[T] protocols

import core.opt.{Opt}

type Iter[T] {
    func next(self): Opt[T]
}

type ToIter[T] {
    func to_iter(self): Iter[T]
}
