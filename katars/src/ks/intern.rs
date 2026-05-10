//! Interning tables for immutable shared data.
//!
//! Interned values are deduplicated via hash lookup and shared via `Arc`.
//! Equality checks get a pointer-equality fast path: if two `Arc`s point
//! to the same allocation, the values are equal without content comparison.

use std::collections::HashSet;
use std::sync::Arc;

use num_bigint::BigInt;

/// Interning tables for all immutable value types.
#[derive(Debug, Default)]
pub struct InternTables {
    strings: HashSet<Arc<str>>,
    bins: HashSet<Arc<[u8]>>,
    // Public API: scaffold for BigInt interning, mirrors `strings` / `bins`.
    #[allow(dead_code)]
    ints: HashSet<Arc<BigInt>>,
    // Future: tups: HashSet<Arc<[Value]>>
}

impl InternTables {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Strings ───────────────────────────────────────────────

    /// Intern a string literal — deduplicates identical content.
    pub fn intern_str(&mut self, s: &str) -> Arc<str> {
        if let Some(existing) = self.strings.get(s) {
            Arc::clone(existing)
        } else {
            let arc: Arc<str> = Arc::from(s);
            self.strings.insert(Arc::clone(&arc));
            arc
        }
    }

    /// Wrap a computed string in Arc (no dedup — just cheap cloning).
    pub fn make_str(s: String) -> Arc<str> {
        Arc::from(s)
    }

    // ── Binary data ───────────────────────────────────────────

    /// Intern binary data — deduplicates identical byte sequences.
    pub fn intern_bin(&mut self, bytes: Vec<u8>) -> Arc<[u8]> {
        if let Some(existing) = self.bins.get(bytes.as_slice()) {
            Arc::clone(existing)
        } else {
            let arc: Arc<[u8]> = bytes.into();
            self.bins.insert(Arc::clone(&arc));
            arc
        }
    }

    // ── Integers ──────────────────────────────────────────────

    // Public API: scaffold for BigInt interning, mirrors `intern_str` / `make_str`.
    #[allow(dead_code)]
    /// Intern a BigInt — deduplicates identical values.
    pub fn intern_int(&mut self, n: BigInt) -> Arc<BigInt> {
        if let Some(existing) = self.ints.get(&n) {
            Arc::clone(existing)
        } else {
            let arc = Arc::new(n);
            self.ints.insert(Arc::clone(&arc));
            arc
        }
    }

    // Public API: scaffold for BigInt interning, mirrors `intern_str` / `make_str`.
    #[allow(dead_code)]
    /// Wrap a BigInt in Arc (no dedup).
    pub fn make_int(n: BigInt) -> Arc<BigInt> {
        Arc::new(n)
    }
}
