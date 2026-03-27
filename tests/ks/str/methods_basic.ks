# Str.len() returns codepoint count, not byte count
let s = "hello"
print(s.len())
print(s.contains("ell"))
print(s.contains("xyz"))
print(s.starts_with("hel"))
print(s.starts_with("xyz"))
print(s.ends_with("llo"))
print(s.ends_with("xyz"))
