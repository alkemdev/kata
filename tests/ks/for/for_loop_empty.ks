# for loop over empty Arr — body never executes
import mem.{Ptr, Buf, heap}

let a = Arr[Int] { buf: Buf[Int] { ptr: Ptr[Int] { raw: heap.make(0) }, cap: 0 }, len: 0 }

for x in a {
    print(x)
}
print("done")
