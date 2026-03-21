# Fibonacci sequence — print the first 20 numbers

func fib(n: Int): Int {
    if n <= 1 {
        ret n
    }
    let a = 0
    let b = 1
    let i = 2
    while i <= n {
        let tmp = b
        b = a + b
        a = tmp
        i = i + 1
    }
    ret b
}

let i = 0
while i < 20 {
    print(fib(i))
    i = i + 1
}
