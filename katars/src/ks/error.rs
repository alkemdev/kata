use std::fmt;

use super::ast::Span;

/// A runtime error with optional source location for ariadne rendering.
#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
    pub span: Option<Span>,
    pub labels: Vec<(Span, String)>,
}

impl RuntimeError {
    /// Create a span-less runtime error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            labels: Vec::new(),
        }
    }

    /// Attach a primary span. No-op if already set (innermost wins).
    pub fn at(mut self, span: Span) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }

    /// Add a secondary annotation label.
    #[allow(dead_code)]
    pub fn label(mut self, span: Span, msg: impl Into<String>) -> Self {
        self.labels.push((span, msg.into()));
        self
    }
}

/// Auto-convert bare `String` errors (from inner methods) into span-less RuntimeErrors.
impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// Display just the message — used by tests for substring matching.
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
