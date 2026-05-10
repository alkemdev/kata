# \uD800 is a UTF-16 surrogate, not a valid Unicode scalar.
# char::from_u32 returns None, so the lexer rejects the token.
# This surfaces as a parse error ("<invalid>" token).
print("\uD800")
