# TODO: Audit and fix the TUI REPL

The REPL exists at `katars/src/tui/mod.rs` (~209 lines, ratatui-based) but hasn't been tested against recent language changes.

Verify:
- Does it still build and launch? (`cargo run -- repl`)
- Does it handle all new syntax? (bail, cont, match, ?, !, array literals, indexing)
- Does it load the prelude correctly?
- Does it handle multi-line input? (func definitions, if/else, match)
- Does it display errors with ariadne rendering?
- Does it support `import`?

Known concerns:
- REPL executes each line as a top-level statement — `?` and `ret` would error ("outside of function")
- Multi-line constructs need some delimiter or continuation detection
- Error rendering may need the full source, not just the current line
