# Specialized impl: methods only apply to Wrapper[Int], not other instantiations
kind Wrapper[T] { val: T }

impl Wrapper[Int] {
    func doubled(self): Int {
        ret self.val * 2
    }
}

let w = Wrapper[Int] { val: 21 }
print(w.doubled())
