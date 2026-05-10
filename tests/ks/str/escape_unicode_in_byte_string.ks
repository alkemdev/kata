# In b"..." / b'...', \u and \U expand to UTF-8 bytes of the codepoint.
# e (U+00E9) is 2 bytes in UTF-8: c3 a9
# emoji (U+1F600) is 4 bytes in UTF-8: f0 9f 98 80
print(b"\u00e9")
print(b'\u00e9')
print(b"\U0001F600")
print(b'\U0001F600')
