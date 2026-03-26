# Specialized impl on Wrapper[Int] should not be available on Wrapper[Str]
kind Wrapper[T] { val: T }

impl Wrapper[Int] {
    func doubled(self): Int {
        ret self.val * 2
    }
}

let w = Wrapper[Str] { val: "hi" }
w.doubled()
