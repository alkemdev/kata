# Stress 04: Recursion — factorial, fibonacci, ackermann (small), mutual recursion.
# Tests stack handling and tail-position correctness.

func fact(n: Int): Int {
    if n <= 1 {
        ret 1
    }
    ret n * fact(n - 1)
}

func fib(n: Int): Int {
    if n < 2 {
        ret n
    }
    ret fib(n - 1) + fib(n - 2)
}

# Ackermann — exercise deep recursion. Small (m=2,n=3) is harmless.
func ack(m: Int, n: Int): Int {
    if m == 0 {
        ret n + 1
    }
    if n == 0 {
        ret ack(m - 1, 1)
    }
    ret ack(m - 1, ack(m, n - 1))
}

# Mutual recursion. KataScript hoists top-level func defs so order shouldn't matter.
func is_even(n: Int): Bool {
    if n == 0 {
        ret true
    }
    ret is_odd(n - 1)
}

func is_odd(n: Int): Bool {
    if n == 0 {
        ret false
    }
    ret is_even(n - 1)
}

print("fact(10) = {fact(10)}")
print("fact(20) = {fact(20)}")
print("fib(15) = {fib(15)}")
print("ack(2, 3) = {ack(2, 3)}")
print("ack(3, 3) = {ack(3, 3)}")
print("is_even(10) = {is_even(10)}")
print("is_odd(7) = {is_odd(7)}")
print("is_even(0) = {is_even(0)}")
