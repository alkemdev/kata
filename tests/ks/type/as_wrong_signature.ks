type Summable {
    func sum(self, other: Int): Int
}

kind Num { val: Int }

impl Num as Summable {
    func sum(self): Int {
        ret self.val
    }
}