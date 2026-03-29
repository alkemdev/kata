# Map: insert enough entries to trigger growth
import mem.{Ptr, Buf, heap}

func make_map(cap: Int): Map[Int, Int] {
    let raw = heap.make(cap)
    let i = 0
    while i < cap {
        unsafe { mem.write(raw, i, Slot[Int, Int].Empty) }
        i = i + 1
    }
    ret Map[Int, Int] {
        slots: Arr[Slot[Int, Int]] {
            buf: Buf[Slot[Int, Int]] { ptr: Ptr[Slot[Int, Int]] { raw: raw }, cap: cap },
            len: cap,
        },
        count: 0,
        cap: cap,
    }
}

let m = make_map(4)
m.set(1, 10)
m.set(2, 20)
m.set(3, 30)
m.set(4, 40)
m.set(5, 50)
print(m.len())
print(m[1])
print(m[5])
