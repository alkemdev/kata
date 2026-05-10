//! Module loading and `import` statement execution.
//!
//! Handles both `import a.b.c` (scoped) and `import a.b.{X, Y}`
//! (selective). Loads embedded standard library sources on first use
//! and caches the resulting `ModuleId` in `loaded_modules`.

use std::io::Write;

use crate::ks::ast::Spanned;
use crate::ks::error::{ErrorKind, RuntimeError};
use crate::ks::native;
use crate::ks::value::Value;

use super::Interpreter;

impl Interpreter {
    pub(super) fn exec_import(
        &mut self,
        path: &[Spanned<String>],
        names: Option<&[Spanned<String>]>,
        out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        // Resolve the module, loading it if necessary.
        let module_id = self.resolve_module_path(path, out)?;
        let module_key = path
            .iter()
            .map(|s| s.node.as_str())
            .collect::<Vec<_>>()
            .join(".");

        match names {
            // Selective: `import mem.{Ptr, Buf}` — pull names into scope.
            Some(selected) => {
                let module = self.native_registry.get_module(module_id);
                let mut exports = Vec::new();
                for spanned_name in selected {
                    let name = &spanned_name.node;
                    let val = module.entries.get(name.as_str()).cloned().ok_or_else(|| {
                        RuntimeError::new(ErrorKind::ModuleNoExport {
                            module: module_key.clone(),
                            name: name.clone(),
                        })
                        .at(spanned_name.span)
                    })?;
                    exports.push((name.clone(), val));
                }
                for (name, val) in exports {
                    self.set(name, val);
                }
            }
            // Scoped: `import mem` — add to the module tree in scope.
            None => {
                let path_strs: Vec<&str> = path.iter().map(|s| s.node.as_str()).collect();
                self.ensure_module_path(&path_strs, module_id);
            }
        }

        Ok(())
    }

    /// Resolve a dotted module path segment by segment.
    /// Loads the module from embedded source if not already loaded.
    /// Errors point to the exact segment that failed.
    fn resolve_module_path(
        &mut self,
        path: &[Spanned<String>],
        out: &mut impl Write,
    ) -> Result<native::ModuleId, RuntimeError> {
        let module_key = path
            .iter()
            .map(|s| s.node.as_str())
            .collect::<Vec<_>>()
            .join(".");

        // Return cached if already loaded.
        if let Some(&mid) = self.loaded_modules.get(&module_key) {
            return Ok(mid);
        }

        // Look up the embedded source. If not found, walk the path
        // segment by segment to find which one fails.
        let source = match self.std_modules.get(&module_key) {
            Some(s) => s,
            None => {
                let mut existing = String::new();
                for seg in path.iter() {
                    let partial = if existing.is_empty() {
                        seg.node.clone()
                    } else {
                        format!("{existing}.{}", seg.node)
                    };
                    // A segment is "known" if it has embedded source, is loaded,
                    // or exists as a module value in scope (e.g., native `ops`).
                    let known = self.std_modules.contains_key(&partial)
                        || self.loaded_modules.contains_key(&partial)
                        || matches!(self.get(&partial), Some(Value::Module(_)));
                    if !known {
                        return Err(RuntimeError::new(ErrorKind::ModuleNoExport {
                            module: if existing.is_empty() {
                                "<root>".into()
                            } else {
                                existing
                            },
                            name: seg.node.clone(),
                        })
                        .at(seg.span));
                    }
                    existing = partial;
                }
                // All segments known but no embedded source. Check if it's
                // already a native module in scope (e.g., `import ops`).
                if let Some(Value::Module(mid)) = self.get(&module_key) {
                    return Ok(mid);
                }
                return Err(RuntimeError::new(ErrorKind::ModuleError {
                    module: module_key.clone(),
                    detail: "not a loadable module".into(),
                })
                .at(path.last().unwrap().span));
            }
        };

        // Parse and execute in a fresh scope to collect exports.
        let filename = format!("<{module_key}>");
        let program =
            crate::ks::parser::parse(source, &filename).map_err(|()| -> RuntimeError {
                ErrorKind::ModuleError {
                    module: module_key.clone(),
                    detail: "failed to parse".into(),
                }
                .into()
            })?;

        // Push a fresh frame, run the module body, and ALWAYS pop the frame
        // before propagating either outcome. If `exec_program` errors, the
        // bare `?` from the previous form would skip the pop and leak the
        // module's frame onto the call stack — corrupting every subsequent
        // lookup.
        self.push_scope();
        let exec_result = self.exec_program(&program, None, out);
        let frame = self.call_stack.pop().unwrap();
        exec_result.map_err(|e| -> RuntimeError {
            ErrorKind::ModuleError {
                module: module_key.clone(),
                detail: e.to_string(),
            }
            .into()
        })?;

        // Collect the scope into a module. If a native module already exists
        // for this path (e.g., mem has native intrinsics), merge KS exports
        // into it rather than replacing it.
        let module_id = self
            .find_existing_module(&module_key)
            .unwrap_or_else(|| self.native_registry.create_module(&module_key));
        for (name, value) in frame.drain() {
            self.native_registry.add_value(module_id, name, value);
        }

        self.loaded_modules
            .insert(module_key.to_string(), module_id);
        Ok(module_id)
    }

    /// Walk a dotted module key (e.g., "mem") through the existing module
    /// tree to find a pre-existing native module. Returns None if any segment
    /// is missing.
    fn find_existing_module(&self, key: &str) -> Option<native::ModuleId> {
        let segments: Vec<&str> = key.split('.').collect();
        let mut current = match self.get(segments[0])? {
            Value::Module(mid) => mid,
            _ => return None,
        };
        for &seg in &segments[1..] {
            current = self.native_registry.find_submodule(current, seg)?;
        }
        Some(current)
    }

    /// Ensure a dotted path exists in scope as nested modules.
    /// Reuses existing modules (including native `ops` and `mem`)
    /// instead of creating new ones that would shadow them.
    fn ensure_module_path(&mut self, path: &[&str], leaf_id: native::ModuleId) {
        if path.is_empty() {
            return;
        }

        // Get or create the root module in scope.
        let root_id = if let Some(Value::Module(mid)) = self.get(path[0]) {
            mid
        } else {
            let mid = self.native_registry.create_module(path[0]);
            self.set(path[0].to_string(), Value::Module(mid));
            mid
        };

        if path.len() == 1 {
            // Single segment: the root IS the leaf. Already set.
            // But if we loaded a new module, we need to merge or replace.
            // For now, just set it.
            self.set(path[0].to_string(), Value::Module(leaf_id));
            return;
        }

        // Walk intermediate segments, creating modules as needed.
        let mut current = root_id;
        for &segment in &path[1..path.len() - 1] {
            let module = self.native_registry.get_module(current);
            if let Some(Value::Module(mid)) = module.entries.get(segment) {
                current = *mid;
            } else {
                let mid = self.native_registry.create_module(segment);
                self.native_registry.add_submodule(current, segment, mid);
                current = mid;
            }
        }

        // Add the leaf module to its parent.
        let leaf_name = path[path.len() - 1];
        self.native_registry
            .add_submodule(current, leaf_name, leaf_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ks::ast::Spanned;

    /// A failing module body (parsed and exec'd from a `std_modules` entry)
    /// must not leak its scope frame onto the interpreter's call stack.
    /// Pre-fix, `exec_program?` would propagate without popping, leaving
    /// the module's frame on top of the call stack and corrupting every
    /// subsequent variable lookup in the calling program.
    #[test]
    fn failing_import_does_not_leak_scope_frame() {
        let mut interp = Interpreter::new();
        // Inject a faulty embedded module: division by zero at top level.
        interp
            .std_modules
            .insert("brokenmod".into(), "let _ = 1 / 0;");
        let baseline = interp.call_stack.len();

        let path = [Spanned::new("brokenmod".to_string(), (0, 9))];
        let mut buf = Vec::new();
        let result = interp.exec_import(&path, None, &mut buf);

        // The import must surface as an error mentioning the module and
        // the underlying cause. Detail wording (`DivisionByZero` Debug
        // form vs. friendly text) is governed by `RuntimeError`'s Display
        // impl, not this fix — assert only that both substrings are present.
        assert!(result.is_err(), "expected import to fail");
        let err_msg = result.unwrap_err().kind.format_with(&interp.types);
        assert!(
            err_msg.contains("brokenmod") && err_msg.contains("DivisionByZero"),
            "expected ModuleError mentioning brokenmod and DivisionByZero, got: {err_msg}"
        );

        // And the call stack must be back where it was — no leaked frame.
        assert_eq!(
            interp.call_stack.len(),
            baseline,
            "scope frame leaked: call_stack grew from {baseline} to {} after failed import",
            interp.call_stack.len(),
        );
    }
}
