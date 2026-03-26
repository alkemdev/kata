# Consume an iterator via interface-typed parameter
func first(iter: Iter[Int]) {
    print(iter.next())
}

let a = [10, 20, 30]
first(a.to_iter())
