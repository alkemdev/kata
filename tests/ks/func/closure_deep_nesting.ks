# Three levels of nested funcs — innermost mutates the outermost's
# binding. The slot is shared all the way down via `closure_scope`,
# so the write at depth 3 propagates back to the top-level frame.
let n = 0
func a() {
    func b() {
        func c() {
            n = n + 1
        }
        c()
    }
    b()
}
a()
a()
a()
print(n)
