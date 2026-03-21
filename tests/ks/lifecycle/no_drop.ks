# Types without Drop are not affected
kind Plain { x: Int }

func test() {
    let p = Plain { x: 42 }
    print(p.x)
}

test()
print("done")
