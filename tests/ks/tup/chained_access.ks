# Chained numeric field access: t.0.0, t.0.1, etc.
# (Lexer fuses N.M into a single Num; parser splits on the dot.)
let deep = (((10,),),)
print(deep.0.0.0)
let pairs = ((1, 2), (3, 4), (5, 6))
print(pairs.0.0)
print(pairs.1.1)
print(pairs.2.0)
