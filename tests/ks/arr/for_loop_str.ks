# for loop over Arr[Str]
let words = Arr[Str] { buf: Buf[Str] { ptr: Ptr[Str] { raw: heap.make(4) }, cap: 4 }, len: 0 }
words.push("hello")
words.push("world")

for w in words {
    print(w)
}
