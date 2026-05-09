//! Method-name interning. Maps source-level method names like `"to_iter"`,
//! `"next"`, `"hash"`, `"new"` to compact `MethodId(u32)` handles so the
//! method-dispatch hot path is a u32-keyed hash hit, not a string compare.
//!
//! The interner is owned by `Interpreter`. Method names are interned at
//! `register_impl_methods` time (insertion into the method tables); the
//! lookup path only *resolves* via the interner — never inserts — so the
//! per-call overhead is one HashMap lookup on a `&str` key.
//!
//! Protocol method names (`Drop::drop`, `ToIter::to_iter`, …) are
//! pre-interned at `Interpreter::new()` and exposed through
//! `ProtocolMethods` for handle-based dispatch from the runtime.

use std::collections::HashMap;
use std::sync::Arc;

use super::Protocol;

/// Handle for a method name. Globally unique per `MethodInterner`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId(u32);

impl MethodId {
    /// Raw integer for debug/diagnostic use only — never compare across
    /// different `MethodInterner` instances.
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// Bidirectional name ⇄ id table.
#[derive(Debug, Default)]
pub struct MethodInterner {
    names: Vec<Arc<str>>,
    indices: HashMap<Arc<str>, MethodId>,
}

impl MethodInterner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a name, inserting it if absent. Mutating.
    pub fn intern(&mut self, name: &str) -> MethodId {
        if let Some(&id) = self.indices.get(name) {
            return id;
        }
        let arc: Arc<str> = Arc::from(name);
        let id = MethodId(self.names.len() as u32);
        self.names.push(arc.clone());
        self.indices.insert(arc, id);
        id
    }

    /// Look up a name without inserting. Read-only.
    pub fn lookup(&self, name: &str) -> Option<MethodId> {
        self.indices.get(name).copied()
    }

    /// Recover the source-level name for diagnostics.
    pub fn name(&self, id: MethodId) -> &str {
        &self.names[id.0 as usize]
    }
}

/// Pre-interned `MethodId`s for the language-level protocols. Populated at
/// `Interpreter::new()` so runtime dispatch never re-interns these names.
#[derive(Debug, Clone, Copy)]
pub struct ProtocolMethods {
    pub to_iter: MethodId,
    pub next: MethodId,
    pub drop: MethodId,
    pub get_item: MethodId,
    pub set_item: MethodId,
}

impl ProtocolMethods {
    pub fn new(interner: &mut MethodInterner) -> Self {
        Self {
            to_iter: interner.intern(Protocol::ToIter.method_name()),
            next: interner.intern(Protocol::Next.method_name()),
            drop: interner.intern(Protocol::Drop.method_name()),
            get_item: interner.intern(Protocol::GetItem.method_name()),
            set_item: interner.intern(Protocol::SetItem.method_name()),
        }
    }

    /// Lookup the cached `MethodId` for a `Protocol`.
    pub fn id(&self, p: Protocol) -> MethodId {
        match p {
            Protocol::ToIter => self.to_iter,
            Protocol::Next => self.next,
            Protocol::Drop => self.drop,
            Protocol::GetItem => self.get_item,
            Protocol::SetItem => self.set_item,
        }
    }
}
