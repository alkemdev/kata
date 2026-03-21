# Nested generic field types: Res[Opt[T], Str]
kind Container[T] { result: Res[Opt[T], Str] }

let c = Container[Int] { result: Res[Opt[Int], Str].Ok(Opt[Int].Some(42)) }
print(c.result)
