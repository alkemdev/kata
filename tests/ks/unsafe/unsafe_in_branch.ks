# Only the taken branch's unsafe gates mem ops; the untaken branch is irrelevant
let cond = true
if cond {
    unsafe {
        let p = mem.alloc(1)
        mem.write(p, 0, 5)
        print(mem.read(p, 0))
        mem.free(p)
    }
} else {
    print("else-branch")
}
print("after-if")
