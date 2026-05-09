//! Protocol method names, stdlib coupling constants, control-flow enum,
//! literal-comparison helpers, and the `eval!` macro that the interpreter
//! uses to evaluate sub-expressions while propagating early returns.
//!
//! Lives in its own module so the rest of the interpreter — `expr`, `stmt`,
//! `match_`, `access`, etc. — can share these primitives without pulling
//! in the full `Interpreter` struct.

use num_bigint::BigInt;

use crate::ks::value::Value;

// ── Protocol methods ────────────────────────────────────────────────────────

/// Protocol method names — methods the interpreter calls on user types to
/// implement language features (iteration, indexing, drop semantics).
/// Kept as a typed enum rather than scattered string consts so the dispatch
/// is type-checked and refactoring-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// `.to_iter()` on iterables — returns an iterator value.
    ToIter,
    /// `.next()` on iterators — returns `Opt[T]`.
    Next,
    /// `.drop()` called when a value goes out of scope.
    Drop,
    /// `.get_item(key)` for `a[key]` reads.
    GetItem,
    /// `.set_item(key, val)` for `a[key] = val` writes.
    SetItem,
}

impl Protocol {
    /// The method name as it appears in source code.
    pub fn method_name(self) -> &'static str {
        match self {
            Protocol::ToIter => "to_iter",
            Protocol::Next => "next",
            Protocol::Drop => "drop",
            Protocol::GetItem => "get_item",
            Protocol::SetItem => "set_item",
        }
    }
}

// ── Stdlib coupling ──────────────────────────────────────────────────────────
//
// Names from the standard library that the interpreter has hardcoded
// knowledge of. The interpreter knows that `Drop` is the conformance
// interface for the drop protocol, that `Val`/`Non` are the variants of
// `Opt`, and that `self` is the receiver parameter name. Migrating these
// to handles would mean caching TypeIds/variant indices at registry init;
// out of scope for now.

pub(super) const SELF_PARAM: &str = "self";
pub(super) const VARIANT_VAL: &str = "Val";
pub(super) const VARIANT_NONE: &str = "Non";
pub(super) const INTERFACE_DROP: &str = "Drop";

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parse an integer literal string, handling decimal, hex (0x), and binary (0b).
pub(super) fn parse_int_literal(s: &str) -> Result<BigInt, String> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        BigInt::parse_bytes(hex.as_bytes(), 16).ok_or_else(|| format!("invalid hex literal: {s}"))
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        BigInt::parse_bytes(bin.as_bytes(), 2).ok_or_else(|| format!("invalid binary literal: {s}"))
    } else {
        s.parse::<BigInt>().map_err(|e| e.to_string())
    }
}

/// Compare a literal AST node to a runtime value for equality.
/// Used by `match` arm matching.
pub(super) fn literal_matches(lit: &crate::ks::ast::Literal, val: &Value) -> bool {
    use crate::ks::ast::Literal;
    match lit {
        Literal::Int(s) => {
            if let Value::Int(n) = val {
                if let Ok(lit_n) = parse_int_literal(s) {
                    return **n == lit_n;
                }
            }
            false
        }
        Literal::Float(s) => {
            if let Value::Float(v) = val {
                if let Ok(lit_f) = s.parse::<f64>() {
                    return *v == lit_f;
                }
            }
            false
        }
        Literal::Str(s) => {
            if let Value::Str(v) = val {
                return &**v == s.as_str();
            }
            false
        }
        Literal::Bool(b) => {
            if let Value::Bool(v) = val {
                return v == b;
            }
            false
        }
        Literal::Nil => matches!(val, Value::Nil),
    }
}

// ── Flow ─────────────────────────────────────────────────────────────────────

/// Outcome of executing a statement or block.
#[derive(Debug)]
pub enum Flow {
    /// Statement completed normally. Carries the value for expression-statements.
    Next(Value),
    /// Explicit `ret` statement. Span is the `ret` keyword.
    Return {
        value: Value,
        span: crate::ks::ast::Span,
    },
    /// `?` operator propagation — unwrap failed, propagating Non/Err upward.
    /// Distinct from Return: different error messages, potentially different
    /// future semantics (e.g., caught by different constructs).
    Propagate {
        value: Value,
        span: crate::ks::ast::Span,
    },
    /// A `bail` was hit; exit the current loop. Span is the `bail` keyword.
    Bail(crate::ks::ast::Span),
    /// A `cont` was hit; skip to the next loop iteration. Span is the keyword.
    Cont(crate::ks::ast::Span),
}

// ── eval! macro ──────────────────────────────────────────────────────────────

/// Evaluate a sub-expression and extract its value.
///
/// Propagates non-normal flows (`Return`, `Break`, `Continue`) instead of
/// collapsing them, so the `?` operator's early-return works correctly in
/// all expression contexts. The macro relies on `Flow`, `RuntimeError`,
/// `ErrorKind`, `FlowMisuse` being in scope at the call site — sibling
/// modules import them from `super` along with this macro.
macro_rules! eval {
    ($self:expr, $expr:expr, $out:expr) => {
        match $self.eval_expr($expr, $out)? {
            $crate::ks::interpreter::types_protocol::Flow::Next(v) => v,
            flow @ ($crate::ks::interpreter::types_protocol::Flow::Return { .. }
            | $crate::ks::interpreter::types_protocol::Flow::Propagate { .. }) => {
                return Ok(flow)
            }
            $crate::ks::interpreter::types_protocol::Flow::Bail(span) => {
                return Err($crate::ks::error::RuntimeError::new(
                    $crate::ks::error::ErrorKind::FlowMisuse(
                        $crate::ks::error::FlowMisuse::BailOutsideLoop,
                    ),
                )
                .at(span))
            }
            $crate::ks::interpreter::types_protocol::Flow::Cont(span) => {
                return Err($crate::ks::error::RuntimeError::new(
                    $crate::ks::error::ErrorKind::FlowMisuse(
                        $crate::ks::error::FlowMisuse::ContOutsideLoop,
                    ),
                )
                .at(span))
            }
        }
    };
}

pub(super) use eval;
