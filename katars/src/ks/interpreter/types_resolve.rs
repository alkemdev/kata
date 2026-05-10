//! Type-name and type-annotation resolution.
//!
//! Maps source-code type expressions (`Int`, `Opt[Int]`, `Tup[Int, Str]`,
//! `(Int, Str)` literals, etc.) onto `TypeId`s in the registry, instantiating
//! generic types as needed. Also handles conformance checks (`conforms_to`,
//! `check_type`) and the AST-level `resolve_type_ann` that lowers a type
//! annotation to a `TypeExpr` (which still needs concretization at impl
//! registration time).

use crate::ks::ast::Expr;
use crate::ks::error::{ErrorKind, NameKind, RuntimeError};
use crate::ks::types::{TypeExpr, TypeId};
use crate::ks::value::Value;

use super::Interpreter;

impl Interpreter {
    /// Resolve a type name string (from source code) to a TypeId.
    pub(super) fn resolve_type(&self, name: &str) -> Result<TypeId, RuntimeError> {
        // Check if it's a value in scope that holds a Type.
        if let Some(Value::Type(tid)) = self.get(name) {
            return Ok(tid);
        }
        // Check the type registry directly.
        self.types.lookup(name).ok_or_else(|| {
            ErrorKind::Undefined {
                kind: NameKind::Type,
                name: name.to_string(),
            }
            .into()
        })
    }

    /// Resolve a type annotation expression to a TypeId.
    /// Handles bare names (`Int`) and parameterized types (`Opt[Int]`).
    pub(super) fn resolve_type_expr(&mut self, expr: &Expr) -> Result<TypeId, RuntimeError> {
        match expr {
            Expr::Name(n) => self.resolve_type(n),
            Expr::Item { object, args } => {
                let base_id = self.resolve_type_expr(&object.node)?;
                let mut type_args = Vec::with_capacity(args.len());
                for a in args {
                    type_args.push(self.resolve_type_expr(&a.node)?);
                }
                self.types
                    .instantiate_by_kind(base_id, type_args)
                    .map_err(Into::into)
            }
            Expr::TupLit { elements } => {
                let mut type_args = Vec::with_capacity(elements.len());
                for e in elements {
                    type_args.push(self.resolve_type_expr(&e.node)?);
                }
                Ok(self.types.instantiate_tup(type_args))
            }
            _ => Err(ErrorKind::Unsupported("unsupported type annotation expression").into()),
        }
    }

    /// Check if a concrete type conforms to an interface type.
    pub(super) fn conforms_to(&self, concrete: TypeId, interface: TypeId) -> bool {
        let concrete_base = self.types.base_type(concrete);
        let interface_base = self.types.base_type(interface);
        self.conformances.contains(&(concrete_base, interface_base))
    }

    /// Type-check a value against an expected type. Returns the value unchanged
    /// if types match, wrapped in AsType if the value's type conforms to an
    /// interface, or an error if neither.
    pub(super) fn check_type(
        &self,
        value: Value,
        expected: TypeId,
    ) -> Result<Value, RuntimeError> {
        let actual = value.type_id();
        if actual == expected {
            return Ok(value);
        }
        if self.conforms_to(actual, expected) {
            return Ok(Value::AsType {
                inner: Box::new(value),
                interface_id: expected,
            });
        }
        Err(ErrorKind::TypeMismatch { expected, actual }.into())
    }

    /// Convert an expression used as a type annotation to a TypeExpr.
    /// Handles bare names (`Int`, `T`), and generic applications
    /// (`Ptr[T]`, `Res[T, E]`).
    pub(super) fn resolve_type_ann(
        &self,
        expr: &Expr,
        type_params: &[String],
    ) -> Result<TypeExpr, RuntimeError> {
        match expr {
            Expr::Name(n) => {
                if let Some(idx) = type_params.iter().position(|p| p == n) {
                    Ok(TypeExpr::Param(idx))
                } else {
                    Ok(TypeExpr::Concrete(self.resolve_type(n)?))
                }
            }
            Expr::Item { object, args } => {
                // Generic type application: e.g., Ptr[T], Opt[T], Res[T, E]
                let base_id = match &object.node {
                    Expr::Name(n) => self.resolve_type(n)?,
                    _ => {
                        return Err(
                            ErrorKind::Unsupported("nested generic base must be a name").into(),
                        )
                    }
                };
                let type_args: Vec<TypeExpr> = args
                    .iter()
                    .map(|a| self.resolve_type_ann(&a.node, type_params))
                    .collect::<Result<_, _>>()?;
                Ok(TypeExpr::Generic {
                    base: base_id,
                    args: type_args,
                })
            }
            _ => Err(ErrorKind::Unsupported("unsupported type annotation").into()),
        }
    }
}
