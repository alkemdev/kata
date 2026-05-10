# Closures capture by shared reference — assigning to a name visible in
# an outer scope writes through the shared slot. The mutation propagates
# both to the outer scope and to other closures sharing the same slot.
let count = 0
func incr() { count = count + 1 }
incr()
incr()
incr()
print(count)
