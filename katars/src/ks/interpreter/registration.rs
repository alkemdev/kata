//! `enum` / `kind` / `type` definitions and `impl` block processing.
//!
//! Each `register_*` function lowers an AST definition into the type
//! registry and binds the new name in scope. Conformance checking
//! verifies that an `impl K as T { … }` block actually satisfies T's
//! method signatures. Impl-block lowering walks `TypePattern`s to
//! handle generic vs. concrete impls (e.g., `impl Arr[@T]` vs.
//! `impl Arr[Byte]`).

use std::sync::Arc;

use indexmap::IndexMap;

use crate::ks::ast::{
    AstFieldDef, AstVariantDef, Expr, FuncDef, MethodSig, Spanned, TypePattern,
};
use crate::ks::error::{ConformanceError, ErrorKind, NameKind, RuntimeError};
use crate::ks::types::{TypeId, VariantDef};
use crate::ks::value::{FuncData, Value};

use super::types_protocol::SELF_PARAM;
use super::{Interpreter, InterfaceDef};

impl Interpreter {
    pub(super) fn register_enum(
        &mut self,
        name: &str,
        type_params: &[String],
        ast_variants: &[AstVariantDef],
    ) -> Result<(), RuntimeError> {
        let mut variants = IndexMap::new();
        for v in ast_variants {
            let fields = v
                .fields
                .iter()
                .map(|f| self.resolve_type_ann(&f.node, type_params))
                .collect::<Result<Vec<_>, _>>()?;
            variants.insert(v.name.node.clone(), VariantDef { fields });
        }

        let type_id = self
            .types
            .register_enum(name.to_string(), type_params.to_vec(), variants);

        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
    }

    pub(super) fn register_struct(
        &mut self,
        name: &str,
        type_params: &[String],
        ast_fields: &[AstFieldDef],
    ) -> Result<(), RuntimeError> {
        let mut fields = IndexMap::new();
        for f in ast_fields {
            let texpr = self.resolve_type_ann(&f.type_ann.node, type_params)?;
            fields.insert(f.name.node.clone(), texpr);
        }

        let type_id = self
            .types
            .register_struct(name.to_string(), type_params.to_vec(), fields);

        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
    }

    pub(super) fn register_interface(
        &mut self,
        name: &str,
        type_params: &[String],
        methods: &[MethodSig],
    ) -> Result<(), RuntimeError> {
        let type_id = self
            .types
            .register_interface(name.to_string(), type_params.to_vec());
        self.set(name.to_string(), Value::Type(type_id));

        self.interfaces.insert(
            name.to_string(),
            InterfaceDef {
                type_params: type_params.to_vec(),
                methods: methods.to_vec(),
            },
        );
        Ok(())
    }

    /// Check that a type's method table satisfies an interface.
    pub(super) fn check_conformance(
        &self,
        type_id: TypeId,
        iface_expr: &Expr,
    ) -> Result<(), RuntimeError> {
        let iface_name = match iface_expr {
            Expr::Name(n) => n.as_str(),
            Expr::Item { object, .. } => {
                if let Expr::Name(n) = &object.node {
                    n.as_str()
                } else {
                    return Err(ErrorKind::Unsupported("invalid interface expression").into());
                }
            }
            _ => return Err(ErrorKind::Unsupported("invalid interface expression").into()),
        };

        let iface = self
            .interfaces
            .get(iface_name)
            .ok_or_else(|| -> RuntimeError {
                ErrorKind::Undefined {
                    kind: NameKind::Interface,
                    name: iface_name.to_string(),
                }
                .into()
            })?
            .clone();

        let type_display = self.types.display_name(type_id);
        let method_table = self.methods.get(&type_id).ok_or_else(|| -> RuntimeError {
            ErrorKind::ConformanceFailure {
                type_name: type_display.clone(),
                iface_name: iface_name.to_string(),
                detail: ConformanceError::TypeHasNoMethods,
            }
            .into()
        })?;

        for sig in &iface.methods {
            // Conformance check: the type must have a method matching each
            // of the interface's signatures. Names are looked up through
            // the interner — if a name was never registered, no method by
            // that name exists, so conformance fails.
            let func = self
                .method_interner
                .lookup(&sig.name.node)
                .and_then(|mid| method_table.get(&mid))
                .ok_or_else(|| -> RuntimeError {
                    ErrorKind::ConformanceFailure {
                        type_name: type_display.clone(),
                        iface_name: iface_name.to_string(),
                        detail: ConformanceError::MissingMethod {
                            method: sig.name.node.clone(),
                        },
                    }
                    .into()
                })?;

            if let Value::Func(f) = func {
                if f.params.len() != sig.params.len() {
                    return Err(ErrorKind::ConformanceFailure {
                        type_name: type_display.clone(),
                        iface_name: iface_name.to_string(),
                        detail: ConformanceError::ParamCountMismatch {
                            method: sig.name.node.clone(),
                            expected: sig.params.len(),
                            actual: f.params.len(),
                        },
                    }
                    .into());
                }
            }
        }

        Ok(())
    }

    // ── Impl registration ────────────────────────────────────────────

    /// Resolve a TypePattern to a TypeId and extract binding names.
    ///
    /// - `Concrete("Int")` → resolve to TypeId, no bindings.
    /// - `Binding("T")` → error (can't impl a bare binding).
    /// - `Apply { base: "Arr", args: [Binding("T")] }` → base TypeId, bindings = ["T"].
    /// - `Apply { base: "Arr", args: [Concrete("Byte")] }` → instantiate Arr[Byte], no bindings.
    pub(super) fn resolve_type_pattern(
        &mut self,
        pattern: &TypePattern,
    ) -> Result<(TypeId, Vec<String>), RuntimeError> {
        match pattern {
            TypePattern::Concrete(name) => {
                let tid = self.resolve_type(name)?;
                Ok((tid, vec![]))
            }
            TypePattern::Binding(_name) => Err(ErrorKind::Unsupported(
                "cannot impl a bare type binding (@T); must be part of a type application",
            )
            .into()),
            TypePattern::Apply { base, args } => {
                let base_id = self.resolve_type(base)?;
                // Collect bindings and check if fully concrete.
                let mut bindings: Vec<String> = Vec::new();
                self.collect_bindings_from_pattern_args(args, &mut bindings)?;

                if bindings.is_empty() {
                    // Fully concrete: instantiate the type.
                    let type_args = args
                        .iter()
                        .map(|a| self.resolve_concrete_pattern(&a.node))
                        .collect::<Result<Vec<_>, _>>()?;
                    let inst_id = self
                        .types
                        .instantiate_by_kind(base_id, type_args)
                        .map_err(|e| -> RuntimeError { e.into() })?;
                    Ok((inst_id, vec![]))
                } else {
                    // Has bindings: store under the base type.
                    Ok((base_id, bindings))
                }
            }
        }
    }

    /// Recursively collect all `@Name` bindings from type pattern args.
    /// Errors on duplicate bindings (for now — unification of repeated bindings is phase 2).
    fn collect_bindings_from_pattern_args(
        &self,
        args: &[Spanned<TypePattern>],
        bindings: &mut Vec<String>,
    ) -> Result<(), RuntimeError> {
        for arg in args {
            match &arg.node {
                TypePattern::Binding(name) => {
                    if !bindings.contains(name) {
                        bindings.push(name.clone());
                    }
                    // Repeated @T is allowed — means unification (same type).
                }
                TypePattern::Concrete(_) => {}
                TypePattern::Apply { args: sub_args, .. } => {
                    self.collect_bindings_from_pattern_args(sub_args, bindings)?;
                }
            }
        }
        Ok(())
    }

    /// Resolve a fully-concrete TypePattern (no bindings) to a TypeId.
    fn resolve_concrete_pattern(&mut self, pattern: &TypePattern) -> Result<TypeId, RuntimeError> {
        match pattern {
            TypePattern::Concrete(name) => self.resolve_type(name),
            TypePattern::Binding(name) => Err(ErrorKind::Undefined {
                kind: NameKind::Type,
                name: name.clone(),
            }
            .into()),
            TypePattern::Apply { base, args } => {
                let base_id = self.resolve_type(base)?;
                let type_args = args
                    .iter()
                    .map(|a| self.resolve_concrete_pattern(&a.node))
                    .collect::<Result<Vec<_>, _>>()?;
                self.types
                    .instantiate_by_kind(base_id, type_args)
                    .map_err(|e| e.into())
            }
        }
    }

    /// Register methods from an impl block under the given TypeId.
    pub(super) fn register_impl_methods(
        &mut self,
        type_id: TypeId,
        type_params: &[String],
        methods: &[Spanned<FuncDef>],
    ) -> Result<(), RuntimeError> {
        // Make `Self` available as a type alias within this impl block.
        self.set("Self".into(), Value::Type(type_id));

        for method in methods {
            let FuncDef {
                name,
                params,
                ret_type,
                body,
            } = &method.node;

            // Static methods (no self) are allowed — they're called on the type value.
            // Instance methods must have self as the first parameter.
            let _is_static = params.is_empty() || params[0].name.node != SELF_PARAM;

            // Resolve params using type_params so generic annotations
            // (e.g., `val: T`) produce TypeExpr::Param(idx).
            let func_params = self.resolve_params_with_type_params(params, type_params)?;

            let ret_texpr = ret_type
                .as_ref()
                .map(|ann| self.resolve_type_ann(&ann.node, type_params))
                .transpose()?;

            let captured = self.capture_scope();
            let func = Value::Func(Arc::new(FuncData {
                params: func_params,
                ret_type: ret_texpr,
                body: body.clone(),
                closure_scope: Some(captured),
            }));

            let method_id = self.method_interner.intern(&name.node);
            self.methods
                .entry(type_id)
                .or_insert_with(IndexMap::new)
                .insert(method_id, func);
        }

        // Remove `Self` so it doesn't leak into surrounding scope.
        self.remove("Self");

        Ok(())
    }
}
