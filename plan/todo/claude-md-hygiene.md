# TODO: CLAUDE.md hygiene pass

## Items

### Remove `Spanned<Expr>` invariant bullet
The specific field names (`Param.type_ann`, `AstFieldDef.type_ann`, etc.) are implementation detail that goes stale when fields change. The "rich data models over string hacks" invariant already covers the principle. Cut the bullet and let the general rule do the work.

### Expand "Keeping docs current" into a checklist
Currently just "update Language status." Should also mention:
- Update the "Not yet" line when a feature moves off it
- Close relevant `plan/prop/` proposals (move to `docs/spec/`)
- Update `plan/roadmap.md` checkboxes

### Document the conformance runner contract
One line explaining: the runner matches `*.ks` to sibling `*.expected` by name, runs the script, and diffs stdout. Saves a trip to `conformance.rs` every time.
