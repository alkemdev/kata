use std::fmt;

use super::ast::{BinOp, Span, UnaryOp};
use super::types::{TypeId, TypeRegistry};

// ── ErrorKind ───────────────────────────────────────────────────────────────

/// Structured runtime error kind. Carries raw data; formatting is deferred
/// to `format_with(&TypeRegistry)` at render time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Variants are infrastructure for Phase 2 conversion.
pub enum ErrorKind {
    Undefined {
        kind: NameKind,
        name: String,
    },
    TypeMismatch {
        expected: TypeId,
        actual: TypeId,
    },
    ArityMismatch {
        target: ArityTarget,
        expected: usize,
        actual: usize,
    },
    WrongTypeKind {
        type_id: TypeId,
        expected: TypeKindExpectation,
    },
    NoMember {
        type_id: TypeId,
        member: String,
        access: MemberAccess,
    },
    BinOpType {
        op: BinOp,
        left: TypeId,
        right: TypeId,
    },
    UnaryOpType {
        op: UnaryOp,
        operand: TypeId,
    },
    DivisionByZero,
    NanComparison,
    MethodDef {
        method: String,
        detail: MethodDefError,
    },
    ConformanceFailure {
        type_name: String,
        iface_name: String,
        detail: ConformanceError,
    },
    FlowMisuse(FlowMisuse),
    Unsupported(&'static str),
    IteratorProtocol(&'static str),
    InvalidLiteral {
        kind: &'static str,
        text: String,
        reason: String,
    },
    UnknownTypeParam {
        param: String,
        context_kind: &'static str,
        context_name: String,
    },
    /// Migration bridge — wraps bare String errors from inner methods.
    Other(String),
}

impl ErrorKind {
    /// Format the error message using the type registry for display names.
    pub fn format_with(&self, types: &TypeRegistry) -> String {
        match self {
            ErrorKind::Undefined { kind, name } => {
                let kind_str = match kind {
                    NameKind::Variable => "variable",
                    NameKind::Type => "type",
                    NameKind::Interface => "interface",
                };
                format!("undefined {kind_str} '{name}'")
            }
            ErrorKind::TypeMismatch { expected, actual } => {
                format!(
                    "type mismatch: expected {}, got {}",
                    types.display_name(*expected),
                    types.display_name(*actual),
                )
            }
            ErrorKind::ArityMismatch {
                target,
                expected,
                actual,
            } => {
                let target_str = match target {
                    ArityTarget::Function => "function".to_string(),
                    ArityTarget::Method => "method".to_string(),
                    ArityTarget::Variant { name } => format!("'{name}'"),
                    ArityTarget::TypeArgs { name } => {
                        return format!(
                            "'{name}' expects {} type argument(s), got {actual}",
                            expected,
                        );
                    }
                    ArityTarget::Builtin { name } => format!("'{name}'"),
                };
                format!("{target_str} expects {expected} argument(s), got {actual}")
            }
            ErrorKind::WrongTypeKind { type_id, expected } => {
                let name = types.display_name(*type_id);
                match expected {
                    TypeKindExpectation::GenericType => format!("'{name}' is not a generic type"),
                    TypeKindExpectation::GenericEnum => format!("'{name}' is not a generic enum"),
                    TypeKindExpectation::GenericStruct => {
                        format!("'{name}' is not a generic struct")
                    }
                    TypeKindExpectation::InstantiatedEnum => {
                        format!("'{name}' is not an instantiated enum")
                    }
                    TypeKindExpectation::StructType => format!("'{name}' is not a struct type"),
                    TypeKindExpectation::Callable => format!("'{name}' is not callable"),
                    TypeKindExpectation::Constructible => {
                        format!("'{name}' is not constructible")
                    }
                    TypeKindExpectation::NotStructType => {
                        format!("'{name}' is not a struct type")
                    }
                }
            }
            ErrorKind::NoMember {
                type_id,
                member,
                access,
            } => {
                let name = types.display_name(*type_id);
                match access {
                    MemberAccess::Variant => format!("'{name}' has no variant '{member}'"),
                    MemberAccess::FieldOrMethod => {
                        format!("'{name}' has no field or method '{member}'")
                    }
                    MemberAccess::Field => format!("'{name}' has no field '{member}'"),
                    MemberAccess::Attr => {
                        format!("cannot access '.{member}' on {name}")
                    }
                    MemberAccess::Method => format!("'{name}' has no method '{member}'"),
                }
            }
            ErrorKind::BinOpType { op, left, right } => {
                format!(
                    "cannot apply '{}' to {} and {}",
                    op.symbol(),
                    types.display_name(*left),
                    types.display_name(*right),
                )
            }
            ErrorKind::UnaryOpType { op, operand } => {
                format!(
                    "cannot apply '{}' to {}",
                    op.symbol(),
                    types.display_name(*operand),
                )
            }
            ErrorKind::DivisionByZero => "division by zero".to_string(),
            ErrorKind::NanComparison => "NaN comparison".to_string(),
            ErrorKind::MethodDef { method, detail } => match detail {
                MethodDefError::MissingSelf => {
                    format!("method '{method}' must take 'self' as first parameter")
                }
                MethodDefError::NotAFunction { type_id } => {
                    format!(
                        "impl body for '{method}' is not a function, got {}",
                        types.display_name(*type_id),
                    )
                }
            },
            ErrorKind::ConformanceFailure {
                type_name,
                iface_name,
                detail,
            } => match detail {
                ConformanceError::MissingMethod { method } => {
                    format!(
                        "type '{type_name}' does not implement '{iface_name}': missing method '{method}'"
                    )
                }
                ConformanceError::ParamCountMismatch {
                    method,
                    expected,
                    actual,
                } => {
                    format!(
                        "type '{type_name}' does not implement '{iface_name}': method '{method}' expects {expected} param(s), got {actual}"
                    )
                }
                ConformanceError::NoMethods => {
                    format!("interface '{iface_name}' has no methods")
                }
            },
            ErrorKind::FlowMisuse(misuse) => match misuse {
                FlowMisuse::BreakOutsideLoop => "break outside of loop".to_string(),
                FlowMisuse::ContinueOutsideLoop => "continue outside of loop".to_string(),
                FlowMisuse::RetOutsideFunction { context } => {
                    format!("ret {context}")
                }
            },
            ErrorKind::Unsupported(msg) => msg.to_string(),
            ErrorKind::IteratorProtocol(msg) => msg.to_string(),
            ErrorKind::InvalidLiteral { kind, text, reason } => {
                format!("invalid {kind} literal '{text}': {reason}")
            }
            ErrorKind::UnknownTypeParam {
                param,
                context_kind,
                context_name,
            } => {
                format!("unknown type parameter '{param}' in {context_kind} '{context_name}'")
            }
            ErrorKind::Other(msg) => msg.clone(),
        }
    }
}

// ── Supporting enums ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NameKind {
    Variable,
    Type,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ArityTarget {
    Function,
    Method,
    Variant { name: String },
    TypeArgs { name: String },
    Builtin { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum TypeKindExpectation {
    GenericType,
    GenericEnum,
    GenericStruct,
    InstantiatedEnum,
    StructType,
    Callable,
    Constructible,
    NotStructType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MemberAccess {
    Variant,
    FieldOrMethod,
    Field,
    Attr,
    Method,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MethodDefError {
    MissingSelf,
    NotAFunction { type_id: TypeId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ConformanceError {
    MissingMethod {
        method: String,
    },
    ParamCountMismatch {
        method: String,
        expected: usize,
        actual: usize,
    },
    NoMethods,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FlowMisuse {
    BreakOutsideLoop,
    ContinueOutsideLoop,
    RetOutsideFunction { context: String },
}

// ── RuntimeError ────────────────────────────────────────────────────────────

/// A runtime error with optional source location for ariadne rendering.
#[derive(Debug)]
pub struct RuntimeError {
    pub kind: ErrorKind,
    pub span: Option<Span>,
    pub labels: Vec<(Span, String)>,
}

impl RuntimeError {
    /// Create a span-less runtime error from an ErrorKind.
    pub fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            span: None,
            labels: Vec::new(),
        }
    }

    /// Format the message using the type registry.
    #[allow(dead_code)]
    pub fn message(&self, types: &TypeRegistry) -> String {
        self.kind.format_with(types)
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

/// Migration bridge: auto-convert bare `String` errors into `ErrorKind::Other`.
impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Other(message))
    }
}

/// Display without a registry — for `Other` variants this is lossless;
/// for structured variants it falls back to Debug.
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Other(msg) => write!(f, "{msg}"),
            other => write!(f, "{other:?}"),
        }
    }
}
