# Match on string literals
let cmd = "quit"
match cmd {
    "help" -> print("showing help"),
    "quit" -> print("goodbye"),
    _ -> print("unknown command"),
}
