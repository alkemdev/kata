# Hash is deterministic — same value hashes the same
print(42.hash() == 42.hash())
print("hello".hash() == "hello".hash())
print(true.hash() == true.hash())
print(U32(99).hash() == U32(99).hash())

# Different values (usually) hash differently
print(42.hash() != 43.hash())
print("hello".hash() != "world".hash())
