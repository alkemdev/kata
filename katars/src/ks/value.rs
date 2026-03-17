use std::fmt;

use indexmap::IndexMap;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

use super::ast::{Spanned, Stmt};
use super::types::{prim, TypeId, TypeRegistry};

// ── Value ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Nil,
    Bool(bool),
    Int(BigInt),
    Float(f64),
    Str(String),
    Bin(Vec<u8>),
    Func {
        params: Vec<FuncParam>,
        ret_type: Option<TypeId>,
        body: Vec<Spanned<Stmt>>,
    },
    Enum {
        type_id: TypeId,
        variant_idx: u32,
        fields: Vec<Value>,
    },
    /// A type value — types are first-class.
    Type(TypeId),
    /// A variant constructor — produced by `Opt[Int].Some`, callable to produce an Enum.
    VariantConstructor {
        type_id: TypeId,
        variant_idx: u32,
        field_types: Vec<TypeId>,
    },
    /// A struct value — e.g., `Point { x: 1.0, y: 2.0 }`.
    Struct {
        type_id: TypeId,
        fields: IndexMap<String, Value>,
    },
    /// A bound method — a Func with `self` already captured.
    BoundMethod {
        receiver: Box<Value>,
        func: Box<Value>,
    },
    /// A namespace value — e.g., `std`, `std.ops`.
    Namespace(String),
    /// A built-in function — e.g., `std.ops.add`.
    BuiltinFn(String),
}

/// A function parameter with an optional type annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncParam {
    pub name: String,
    pub type_id: Option<TypeId>,
}

impl Value {
    /// Get the TypeId for this value's type.
    pub fn type_id(&self) -> TypeId {
        match self {
            Value::Nil => prim::NIL,
            Value::Bool(_) => prim::BOOL,
            Value::Int(_) => prim::INT,
            Value::Float(_) => prim::FLOAT,
            Value::Str(_) => prim::STR,
            Value::Bin(_) => prim::BIN,
            Value::Func { .. } => prim::FUNC,
            Value::Enum { type_id, .. } => *type_id,
            Value::Struct { type_id, .. } => *type_id,
            Value::BoundMethod { .. } => prim::FUNC,
            Value::Type(_) => prim::TYPE,
            Value::VariantConstructor { .. } => prim::FUNC,
            Value::Namespace(_) => prim::NIL,
            Value::BuiltinFn(_) => prim::FUNC,
        }
    }

    /// Format this value for display, using the type registry for enum names.
    pub fn display(&self, types: &TypeRegistry) -> String {
        match self {
            Value::Nil => "nil".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format!("{n}"),
            Value::Str(s) => s.clone(),
            Value::Bin(b) => format!("<bin:{} bytes>", b.len()),
            Value::Func { params, .. } => {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                format!("<func({})>", names.join(", "))
            }
            Value::Enum {
                type_id,
                variant_idx,
                fields,
            } => {
                let variant_name = types.variant_name(*type_id, *variant_idx);
                if fields.is_empty() {
                    variant_name.to_string()
                } else {
                    let inner: Vec<String> = fields.iter().map(|v| v.display(types)).collect();
                    format!("{variant_name}({})", inner.join(", "))
                }
            }
            Value::Struct { type_id, fields } => {
                let type_name = types.display_name(*type_id);
                if fields.is_empty() {
                    format!("{type_name} {{}}")
                } else {
                    let inner: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", v.display(types)))
                        .collect();
                    format!("{type_name} {{ {} }}", inner.join(", "))
                }
            }
            Value::Type(tid) => types.display_name(*tid),
            Value::VariantConstructor {
                type_id,
                variant_idx,
                ..
            } => {
                let variant_name = types.variant_name(*type_id, *variant_idx);
                let type_name = types.display_name(*type_id);
                format!("<constructor {type_name}.{variant_name}>")
            }
            Value::BoundMethod { func, .. } => {
                if let Value::Func { params, .. } = func.as_ref() {
                    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                    format!("<bound-method({})>", names.join(", "))
                } else {
                    "<bound-method(?)>".to_string()
                }
            }
            Value::Namespace(name) => format!("<namespace {name}>"),
            Value::BuiltinFn(name) => format!("<builtin {name}>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bin(a), Value::Bin(b)) => a == b,
            (Value::Type(a), Value::Type(b)) => a == b,
            (
                Value::Enum {
                    type_id: t1,
                    variant_idx: v1,
                    fields: f1,
                },
                Value::Enum {
                    type_id: t2,
                    variant_idx: v2,
                    fields: f2,
                },
            ) => t1 == t2 && v1 == v2 && f1 == f2,
            (
                Value::Struct {
                    type_id: t1,
                    fields: f1,
                },
                Value::Struct {
                    type_id: t2,
                    fields: f2,
                },
            ) => t1 == t2 && f1 == f2,
            (Value::Func { .. }, Value::Func { .. }) => false,
            (Value::BoundMethod { .. }, Value::BoundMethod { .. }) => false,
            (Value::VariantConstructor { .. }, Value::VariantConstructor { .. }) => false,
            (Value::Namespace(a), Value::Namespace(b)) => a == b,
            (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
            _ => false,
        }
    }
}

/// Display without type registry access — fallback for Debug/serde contexts.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Bin(b) => write!(f, "<bin:{} bytes>", b.len()),
            Value::Func { params, .. } => {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                write!(f, "<func({})>", names.join(", "))
            }
            Value::Enum {
                variant_idx,
                fields,
                ..
            } => {
                if fields.is_empty() {
                    write!(f, "<variant:{variant_idx}>")
                } else {
                    let inner: Vec<String> = fields.iter().map(|v| v.to_string()).collect();
                    write!(f, "<variant:{variant_idx}>({})", inner.join(", "))
                }
            }
            Value::Struct { type_id, fields } => {
                let inner: Vec<String> = fields.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                write!(f, "<struct:{type_id} {{ {} }}>", inner.join(", "))
            }
            Value::Type(tid) => write!(f, "<type:{tid}>"),
            Value::VariantConstructor { variant_idx, .. } => {
                write!(f, "<constructor:variant:{variant_idx}>")
            }
            Value::BoundMethod { func, .. } => {
                if let Value::Func { params, .. } = func.as_ref() {
                    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                    write!(f, "<bound-method({})>", names.join(", "))
                } else {
                    write!(f, "<bound-method(?)>")
                }
            }
            Value::Namespace(name) => write!(f, "<namespace {name}>"),
            Value::BuiltinFn(name) => write!(f, "<builtin {name}>"),
        }
    }
}
