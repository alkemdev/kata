# Map: basic set, get, len, has
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
m.set("hello", 1)
m.set("world", 2)
print(m.len())
print(m.get("hello"))
print(m.get("world"))
print(m.get("missing"))
print(m.has("hello"))
print(m.has("missing"))
