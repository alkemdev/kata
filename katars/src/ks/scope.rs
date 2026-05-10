//! Lexical scoping: `Slot`, `Frame`, and `Scope`.
//!
//! Each binding is a `Slot` — a reference-counted, interior-mutable cell
//! holding a `Value`. Frames and captured scopes hold the same `Slot` for
//! a given name, so:
//!
//! 1. **Closures capture by reference.** A closure that mutates an outer
//!    variable writes through the shared slot; the outer scope sees it.
//! 2. **`func`-name self-reference works.** At function definition we
//!    create a placeholder slot, capture the scope (slot included), build
//!    the function with that captured scope, then fill the slot with the
//!    real function. The captured scope already pointed at the slot, so
//!    the function's body can resolve its own name.
//!
//! `Slot` uses `Arc<Mutex<Value>>`. The interpreter is single-threaded but
//! the TUI completer requires `Send + Sync`, which rules out `RefCell`.
//! The mutex is uncontended in practice (only the active interpreter
//! thread touches it); accessors clone the value out (`get`) or replace
//! it atomically (`set`) and never hand out a guard that could outlive a
//! single read.

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;

use super::value::Value;

/// A reference-counted, interior-mutable cell holding a `Value`. Multiple
/// `Frame`s and captured `Scope`s can share the same `Slot`; mutation
/// through any one is visible to all.
#[derive(Debug, Clone)]
pub struct Slot(Arc<Mutex<Value>>);

impl Slot {
    pub fn new(value: Value) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }

    /// Read the slot's current value (clones — `Value`'s heavy variants
    /// are all Arc-wrapped, so this is cheap).
    pub fn get(&self) -> Value {
        self.0.lock().expect("slot mutex poisoned").clone()
    }

    /// Replace the slot's value, returning the old one.
    pub fn set(&self, value: Value) -> Value {
        std::mem::replace(&mut *self.0.lock().expect("slot mutex poisoned"), value)
    }

    /// Mutate the slot's value in place. The closure receives `&mut Value`.
    /// Used for in-place struct-field updates (`a.b = v`).
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut Value) -> R) -> R {
        f(&mut *self.0.lock().expect("slot mutex poisoned"))
    }
}

/// A single mutable binding environment — maps names to slots.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    bindings: IndexMap<String, Slot>,
}

impl Frame {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a binding's value (cloning through the slot).
    pub fn get(&self, name: &str) -> Option<Value> {
        self.bindings.get(name).map(|s| s.get())
    }

    /// Get the slot itself for sharing across scopes.
    pub fn get_slot(&self, name: &str) -> Option<&Slot> {
        self.bindings.get(name)
    }

    /// `let`-style binding: always creates a NEW slot, shadowing any
    /// existing binding with the same name in this frame. Returns the
    /// previous slot's value if there was one (for drop dispatch).
    pub fn set(&mut self, name: String, value: Value) -> Option<Value> {
        let old = self.bindings.insert(name, Slot::new(value));
        old.map(|s| s.get())
    }

    /// Bind a name to an existing slot. Used to wire up the function-self-
    /// reference: place a placeholder slot before capturing the scope, then
    /// `Slot::set` the real function into the same slot afterwards. Other
    /// code paths shouldn't normally need this — prefer `set`.
    pub fn bind_slot(&mut self, name: String, slot: Slot) {
        self.bindings.insert(name, slot);
    }

    /// Assignment-style write: writes through the existing slot if present.
    /// Returns `Some(old_value)` if the binding existed, `None` otherwise.
    pub fn write(&self, name: &str, value: Value) -> Option<Value> {
        self.bindings.get(name).map(|s| s.set(value))
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.bindings.shift_remove(name).map(|s| s.get())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Slot)> {
        self.bindings.iter()
    }

    pub fn drain(self) -> impl Iterator<Item = (String, Value)> {
        self.bindings.into_iter().map(|(k, s)| (k, s.get()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.bindings.keys()
    }
}

/// A frozen scope chain — a frame with a link to its parent. Captured by
/// closures at definition time. Sharing happens at the slot level, not the
/// scope level: cloning an `Arc<Scope>` is cheap and the underlying frames
/// reference slots that may be live in the call stack.
#[derive(Debug, Clone)]
pub struct Scope {
    pub frame: Frame,
    pub parent: Option<Arc<Scope>>,
}

impl Scope {
    /// Look up a name by walking the parent chain.
    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.frame.get(name) {
            return Some(v);
        }
        self.parent.as_ref().and_then(|p| p.lookup(name))
    }

    /// Find the slot for a name (for shared mutation) without dereferencing.
    pub fn lookup_slot(&self, name: &str) -> Option<&Slot> {
        if let Some(s) = self.frame.get_slot(name) {
            return Some(s);
        }
        self.parent.as_ref().and_then(|p| p.lookup_slot(name))
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
        assert_eq!(f.get("x"), Some(str_val("hello")));
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
        assert_eq!(inner_scope.lookup("x"), Some(str_val("inner")));
        // Falls through to outer
        assert_eq!(inner_scope.lookup("y"), Some(str_val("only_outer")));
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

    #[test]
    fn slot_sharing_propagates_writes() {
        // Two frames hold the same slot — write through one, read through other.
        let mut f1 = Frame::new();
        f1.set("x".into(), str_val("v1"));
        let slot = f1.get_slot("x").unwrap().clone();

        let mut f2 = Frame::new();
        f2.bind_slot("x".into(), slot);

        // Mutate via f2's interface; f1 should see the new value.
        f2.write("x", str_val("v2"));
        assert_eq!(f1.get("x"), Some(str_val("v2")));
    }

    #[test]
    fn frame_set_creates_new_slot() {
        // `set` always creates a new slot — shadowing-style. A previously
        // captured handle should not see the new value.
        let mut f = Frame::new();
        f.set("x".into(), str_val("v1"));
        let captured = f.get_slot("x").unwrap().clone();

        // Re-set: creates a new slot at the same name.
        f.set("x".into(), str_val("v2"));

        // The captured slot still points at the old value.
        assert_eq!(captured.get(), str_val("v1"));
        // The frame holds the new value.
        assert_eq!(f.get("x"), Some(str_val("v2")));
    }
}
