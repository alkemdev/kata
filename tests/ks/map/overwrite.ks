# Map: overwriting a key updates the value
import mem.{Ptr, Buf, heap}

func make_map(cap: Int): Map[Str, Int] {
    let raw = heap.make(cap)
    let i = 0
    while i < cap {
        unsafe { mem.write(raw, i, Slot[Str, Int].Empty) }
        i = i + 1
    }
    ret Map[Str, Int] {
        slots: Arr[Slot[Str, Int]] {
            buf: Buf[Slot[Str, Int]] { ptr: Ptr[Slot[Str, Int]] { raw: raw }, cap: cap },
            len: cap,
        },
        count: 0,
        cap: cap,
    }
}

let m = make_map(8)
m.set("a", 1)
m.set("a", 99)
print(m["a"])
print(m.len())
