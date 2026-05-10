# An import that fails must surface its error cleanly. A scope-leak bug
# previously left the failed module's frame on the interpreter's call
# stack, corrupting every subsequent variable lookup. This test locks in
# the user-visible behavior: a missing module aborts the program with a
# clear error pointing at the offending import. The Rust unit test in
# imports.rs covers the call-stack invariant directly.
import this_module_does_not_exist
print("unreachable")
