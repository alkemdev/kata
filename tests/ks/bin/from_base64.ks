# Bin.from_base64() — static method decodes base64 string to Bin
let hello = Bin.from_base64("SGVsbG8=")
print(hello)
print(hello.len())
print(hello[0])

# Empty
let empty = Bin.from_base64("")
print(empty.len())

# Binary data with padding
let data = Bin.from_base64("AQID")
print(data.len())
print(data[0])
print(data[1])
print(data[2])
