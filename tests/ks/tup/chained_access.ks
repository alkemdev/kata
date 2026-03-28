# Chained numeric field access: t._0._0, t._0._1, etc.
let deep = (((10,),),)
print(deep._0._0._0)
let pairs = ((1, 2), (3, 4), (5, 6))
print(pairs._0._0)
print(pairs._1._1)
print(pairs._2._0)
