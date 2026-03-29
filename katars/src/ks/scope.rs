//! Lexical scoping: Frame (mutable bindings) and Scope (frozen chain).
//!
//! A `Frame` is a single mutable binding environment.
//! A `Scope` is an immutable chain of frames linked by parent pointers.
//! Functions capture an `Arc<Scope>` at definition time for lexical closures.

use std::sync::Arc;

use indexmap::IndexMap;

use super::value::Value;

/// A single mutable binding environment — maps names to values.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    bindings: IndexMap<String, Value>,
}

impl Frame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.bindings.get_mut(name)
    }

    pub fn set(&mut self, name: String, value: Value) -> Option<Value> {
        self.bindings.insert(name, value)
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.bindings.shift_remove(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.bindings.iter()
    }

    pub fn drain(self) -> impl Iterator<Item = (String, Value)> {
        self.bindings.into_iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.bindings.keys()
    }
}

/// An immutable frozen scope — a frame with a link to its parent.
/// Shared via `Arc` for closure capture.
#[derive(Debug, Clone)]
pub struct Scope {
    pub frame: Frame,
    pub parent: Option<Arc<Scope>>,
}

impl Scope {
    /// Look up a name by walking the parent chain.
    pub fn lookup(&self, name: &str) -> Option<&Value> {
        if let Some(v) = self.frame.get(name) {
            return Some(v);
        }
        self.parent.as_ref().and_then(|p| p.lookup(name))
    }

    /// Collect all visible names (for REPL completion).
    pub fn visible_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.frame.keys().cloned().collect();
        if let Some(ref parent) = self.parent {
            names.extend(parent.visible_names());
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn str_val(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }

    #[test]
    fn frame_get_set() {
        let mut f = Frame::new();
        assert!(f.get("x").is_none());
        f.set("x".into(), str_val("hello"));
        assert_eq!(f.get("x"), Some(&str_val("hello")));
    }

    #[test]
    fn frame_remove() {
        let mut f = Frame::new();
        f.set("x".into(), str_val("hello"));
        assert!(f.contains("x"));
        f.remove("x");
        assert!(!f.contains("x"));
    }

    #[test]
    fn scope_lookup_chain() {
        let mut inner = Frame::new();
        inner.set("x".into(), str_val("inner"));

        let mut outer = Frame::new();
        outer.set("x".into(), str_val("outer"));
        outer.set("y".into(), str_val("only_outer"));

        let outer_scope = Arc::new(Scope {
            frame: outer,
            parent: None,
        });
        let inner_scope = Scope {
            frame: inner,
            parent: Some(outer_scope),
        };

        // Inner shadows outer
        assert_eq!(inner_scope.lookup("x"), Some(&str_val("inner")));
        // Falls through to outer
        assert_eq!(inner_scope.lookup("y"), Some(&str_val("only_outer")));
        // Not found anywhere
        assert!(inner_scope.lookup("z").is_none());
    }

    #[test]
    fn scope_visible_names() {
        let mut outer = Frame::new();
        outer.set("a".into(), Value::Nil);
        outer.set("b".into(), Value::Nil);

        let mut inner = Frame::new();
        inner.set("c".into(), Value::Nil);

        let outer_scope = Arc::new(Scope {
            frame: outer,
            parent: None,
        });
        let inner_scope = Scope {
            frame: inner,
            parent: Some(outer_scope),
        };

        let names = inner_scope.visible_names();
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"b".to_string()));
        assert!(names.contains(&"c".to_string()));
    }
}
