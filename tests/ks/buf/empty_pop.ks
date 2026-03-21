# Pop on empty buf returns None
let buf = Buf[Int] { ptr: ptr_alloc(0), len: 0, cap: 0 }
print(buf.pop())
print(buf.pop())
