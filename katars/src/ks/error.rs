use std::fmt;

use super::ast::{BinOp, Span, UnaryOp};
use super::types::{TypeId, TypeRegistry};

// ── ErrorKind ───────────────────────────────────────────────────────────────

/// Structured runtime error kind. Carries raw data; formatting is deferred
/// to `format_with(&TypeRegistry)` at render time.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    NoAttr {
        type_id: TypeId,
        attr: String,
        access: AccessKind,
    },
    MissingField {
        type_id: TypeId,
        field: String,
    },
    ExpectedType {
        actual: TypeId,
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
    UnsafeRequired {
        intrinsic: String,
    },
    UseAfterFree,
    NoMatchArm,
    InvalidUnwrap {
        operator: &'static str,
    },
    EmptyArrayLiteral,
    IndexOutOfBounds {
        index: i64,
        len: i64,
    },
    NotIndexable {
        type_id: TypeId,
    },
    /// Explicit `panic(msg)` call from KS code.
    Panic {
        message: String,
    },
    /// Postfix `!` on a Non/Err variant.
    UnwrapFailed {
        type_id: TypeId,
        variant: String,
    },
    /// Module has no export with the given name.
    ModuleNoExport {
        module: String,
        name: String,
    },
    /// Module failed to load (parse or execution error).
    ModuleError {
        module: String,
        detail: String,
    },
    /// Integer value out of representable range.
    IntegerOverflow,
    /// `as` on a type that doesn't conform to the target interface.
    AsNonConforming {
        actual: TypeId,
        interface: TypeId,
    },
    /// Prim constructor value out of range.
    PrimOutOfRange {
        type_name: &'static str,
        detail: String,
    },
    /// Interpreter invariant violation — should never reach user code.
    InternalError(&'static str),

    // ── Pattern errors ──────────────────────────────────────────────────────
    /// Variant pattern names a non-existent variant of the subject's enum.
    /// Example: `Vla(x)` on `Opt[Int]`.
    PatternUnknownVariant {
        type_id: TypeId,
        variant_name: String,
    },
    /// Variant pattern's binding count doesn't match the variant's field count.
    /// Example: `Val(x, y)` on a 1-field variant.
    PatternVariantArity {
        type_id: TypeId,
        variant_name: String,
        expected: usize,
        actual: usize,
    },
    /// Tuple pattern length doesn't match the tuple's arity.
    PatternTupleArity {
        expected: usize,
        actual: usize,
    },
    /// Pattern shape is incompatible with the subject's type.
    /// Example: tuple pattern on `Int`, variant pattern on `Rec`.
    PatternTypeMismatch {
        pattern_kind: PatternKind,
        subject_type: TypeId,
    },
    /// Bare identifier in a match arm matches a variant name of the subject's
    /// enum — almost certainly a forgotten `Name()`. Catches the syntax-migration
    /// gap. The hint is rendered with the subject type for context.
    PatternAmbiguousBinding {
        binding_name: String,
        type_id: TypeId,
    },
    /// Refutable pattern appears in a `let` or `for` binding (which require
    /// irrefutable patterns).
    PatternRefutableInLet {
        kind: PatternKind,
    },
    /// Same binding name appears more than once in a single pattern.
    /// Example: `match p { (x, x) -> ... }` or `let (a, a) = ...`.
    PatternRepeatedBinding {
        name: String,
        first_span: Span,
        repeat_span: Span,
    },
    /// Positional tuple index `.N` applied to a non-tuple value.
    /// Example: `let x = 5; print(x.0)`.
    TupIdxOnNonTuple {
        type_id: TypeId,
        idx: u32,
    },
    /// Attempted to hash a value whose type isn't hashable, or a compound
    /// value (tuple/record/enum) containing an unhashable field.
    /// Examples: `func_value.hash()`, `(some_func, 5).hash()`.
    Unhashable {
        type_id: TypeId,
    },
    /// `Str.to_int()` / `Str.to_float()` etc. failed to parse the input.
    /// `target_type` is the destination prim (Int, Float, …); `input` is
    /// the offending string (truncated to a reasonable length at render).
    ParseError {
        target_type: TypeId,
        input: String,
    },

    /// Escape hatch for errors from external systems where a structured
    /// variant would be busy-work — `print()` IO failures, base64 decode
    /// errors from a third-party crate, etc. **Do not** use this for
    /// kata-internal errors that have a clean home; add a structured
    /// variant instead.
    Other(String),
}

/// Categorizes patterns for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Binding,
    Literal,
    Variant,
    Tuple,
}

impl PatternKind {
    pub fn name(self) -> &'static str {
        match self {
            PatternKind::Wildcard => "wildcard",
            PatternKind::Binding => "binding",
            PatternKind::Literal => "literal",
            PatternKind::Variant => "variant",
            PatternKind::Tuple => "tuple",
        }
    }
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
                            "'{name}' expects {expected} type argument(s), got {actual}",
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
                        format!("cannot construct '{name}' — not a type")
                    }
                    TypeKindExpectation::Indexable => format!("cannot index into {name}"),
                    TypeKindExpectation::ExpectedEnum => {
                        format!("'{name}' is a struct type — construct with `{name} {{ ... }}`")
                    }
                }
            }
            ErrorKind::NoAttr {
                type_id,
                attr,
                access,
            } => {
                let name = types.display_name(*type_id);
                match access {
                    AccessKind::Variant => format!("'{name}' has no variant '{attr}'"),
                    AccessKind::FieldOrMethod => {
                        format!("'{name}' has no field or method '{attr}'")
                    }
                    AccessKind::Field => format!("'{name}' has no field '{attr}'"),
                    AccessKind::Attr => format!("cannot access '.{attr}' on {name}"),
                    AccessKind::Method => format!("'{name}' has no method '{attr}'"),
                }
            }
            ErrorKind::MissingField { type_id, field } => {
                format!(
                    "missing field '{field}' in '{}' construction",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::ExpectedType { actual } => {
                format!(
                    "expected a type argument, got {}",
                    types.display_name(*actual),
                )
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
                    format!("method '{method}' must have 'self' as first parameter")
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
                        "'{type_name}' does not implement '{iface_name}': missing method '{method}'"
                    )
                }
                ConformanceError::ParamCountMismatch {
                    method,
                    expected,
                    actual,
                } => {
                    format!(
                        "'{type_name}' does not implement '{iface_name}': method '{method}' expects {expected} param(s), got {actual}"
                    )
                }
                ConformanceError::TypeHasNoMethods => {
                    format!("'{type_name}' has no methods")
                }
            },
            ErrorKind::FlowMisuse(misuse) => match misuse {
                FlowMisuse::BailOutsideLoop => "bail outside of loop".to_string(),
                FlowMisuse::ContOutsideLoop => "cont outside of loop".to_string(),
                FlowMisuse::RetOutsideFunction => "ret outside of function".to_string(),
                FlowMisuse::PropagateOutsideFunction => {
                    "? operator used outside of function".to_string()
                }
            },
            ErrorKind::Unsupported(msg) => msg.to_string(),
            ErrorKind::IteratorProtocol(msg) => msg.to_string(),
            ErrorKind::InvalidLiteral { kind, text, reason } => {
                format!("invalid {kind} literal '{text}': {reason}")
            }
            ErrorKind::UnsafeRequired { intrinsic } => {
                format!("intrinsic '{intrinsic}' can only be called inside an unsafe block")
            }
            ErrorKind::UseAfterFree => "use of deallocated memory".to_string(),
            ErrorKind::NoMatchArm => "no match arm matched".to_string(),
            ErrorKind::InvalidUnwrap { operator } => {
                format!("{operator} requires an Opt[T] or Res[T, E] value")
            }
            ErrorKind::EmptyArrayLiteral => {
                "empty array literal — cannot infer element type".to_string()
            }
            ErrorKind::IndexOutOfBounds { index, len } => {
                format!("index out of bounds: index is {index} but length is {len}")
            }
            ErrorKind::NotIndexable { type_id } => {
                let name = types.display_name(*type_id);
                format!("'{name}' does not support indexing")
            }
            ErrorKind::Panic { message } => message.clone(),
            ErrorKind::UnwrapFailed { type_id, variant } => {
                let name = types.display_name(*type_id);
                format!("unwrap on {name}.{variant}")
            }
            ErrorKind::ModuleNoExport { module, name } => {
                if module == "<root>" {
                    format!("unknown module '{name}'")
                } else {
                    format!("module '{module}' has no export '{name}'")
                }
            }
            ErrorKind::ModuleError { module, detail } => {
                format!("error in module '{module}': {detail}")
            }
            ErrorKind::AsNonConforming { actual, interface } => {
                format!(
                    "'{}' does not implement '{}'",
                    types.display_name(*actual),
                    types.display_name(*interface),
                )
            }
            ErrorKind::PrimOutOfRange { type_name, detail } => {
                format!("{type_name} value out of range: {detail}")
            }
            ErrorKind::IntegerOverflow => "integer out of representable range".to_string(),
            ErrorKind::InternalError(msg) => format!("internal error: {msg}"),
            ErrorKind::PatternUnknownVariant {
                type_id,
                variant_name,
            } => {
                format!(
                    "no variant '{variant_name}' on type {}",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::PatternVariantArity {
                type_id,
                variant_name,
                expected,
                actual,
            } => {
                format!(
                    "variant '{variant_name}' of {} has {expected} field(s) but pattern has {actual}",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::PatternTupleArity { expected, actual } => {
                format!("tuple pattern has {actual} element(s) but tuple has {expected}")
            }
            ErrorKind::PatternTypeMismatch {
                pattern_kind,
                subject_type,
            } => {
                format!(
                    "{} pattern cannot match {} value",
                    pattern_kind.name(),
                    types.display_name(*subject_type),
                )
            }
            ErrorKind::PatternAmbiguousBinding {
                binding_name,
                type_id,
            } => {
                format!(
                    "'{binding_name}' is a variant of {}; use {binding_name}() to match it as a variant, or rename the binding",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::PatternRefutableInLet { kind } => {
                format!(
                    "{} pattern is refutable; use match instead of let/for",
                    kind.name(),
                )
            }
            ErrorKind::PatternRepeatedBinding { name, .. } => {
                format!("binding '{name}' appears twice in pattern")
            }
            ErrorKind::TupIdxOnNonTuple { type_id, idx } => {
                format!(
                    "cannot apply tuple index .{idx} to {} (positional indexing requires a tuple)",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::Unhashable { type_id } => {
                format!(
                    "value of type {} is not hashable",
                    types.display_name(*type_id),
                )
            }
            ErrorKind::ParseError { target_type, input } => {
                let preview = if input.chars().count() > 32 {
                    let head: String = input.chars().take(29).collect();
                    format!("{head}…")
                } else {
                    input.clone()
                };
                format!(
                    "cannot parse '{preview}' as {}",
                    types.display_name(*target_type),
                )
            }
            ErrorKind::Other(msg) => msg.clone(),
        }
    }
}

// ── Supporting enums ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Variable,
    Type,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    Indexable,
    ExpectedEnum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
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
pub enum ConformanceError {
    MissingMethod {
        method: String,
    },
    ParamCountMismatch {
        method: String,
        expected: usize,
        actual: usize,
    },
    TypeHasNoMethods,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowMisuse {
    BailOutsideLoop,
    ContOutsideLoop,
    RetOutsideFunction,
    PropagateOutsideFunction,
}

// ── RuntimeError ────────────────────────────────────────────────────────────

/// A runtime error with optional source location for ariadne rendering.
#[derive(Debug)]
pub struct RuntimeError {
    pub kind: ErrorKind,
    pub span: Option<Span>,
    pub labels: Vec<(Span, String)>,
    pub help: Option<String>,
    pub note: Option<String>,
}

impl RuntimeError {
    /// Create a span-less runtime error from an ErrorKind.
    pub fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            span: None,
            labels: Vec::new(),
            help: None,
            note: None,
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
    pub fn label(mut self, span: Span, msg: impl Into<String>) -> Self {
        self.labels.push((span, msg.into()));
        self
    }

    /// Add an actionable suggestion (rendered as "help: ..." by ariadne).
    pub fn help(mut self, msg: impl Into<String>) -> Self {
        self.help = Some(msg.into());
        self
    }

    /// Add explanatory context (rendered as "note: ..." by ariadne).
    pub fn note(mut self, msg: impl Into<String>) -> Self {
        self.note = Some(msg.into());
        self
    }
}

/// Auto-convert `ErrorKind` into a span-less RuntimeError.
impl From<ErrorKind> for RuntimeError {
    fn from(kind: ErrorKind) -> Self {
        Self::new(kind)
    }
}

/// Migration bridge: auto-convert bare `String` errors into `ErrorKind::Other`.
impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Other(message))
    }
}

/// Convenience: auto-convert `&str` into `ErrorKind::Other`.
impl From<&str> for RuntimeError {
    fn from(message: &str) -> Self {
        Self::new(ErrorKind::Other(message.to_string()))
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
