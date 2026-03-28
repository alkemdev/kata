# Non-empty tuples are truthy, empty tuple is falsy
print(if (1, 2) { "yes" } else { "no" })
print(if () { "yes" } else { "no" })
