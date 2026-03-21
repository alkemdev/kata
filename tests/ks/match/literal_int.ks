# Match on integer literals
let code = 404
let msg = match code {
    200 -> "ok",
    404 -> "not found",
    500 -> "server error",
    _ -> "unknown",
}
print(msg)
