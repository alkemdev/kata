//! Attribute and item read / write paths.
//!
//! `eval_attr` / `eval_item` resolve `obj.name` and `obj[key]` reads;
//! `exec_attr_assign` / `exec_item_assign` resolve writes. Method-table
//! lookup and call dispatch live in `call.rs` — this file only handles
//! the routing from a postfix operator to whatever helper does the
//! actual work.

use std::io::Write;
use std::sync::Arc;

use crate::ks::ast::{Expr, Spanned};
use crate::ks::error::{AccessKind, ErrorKind, NameKind, RuntimeError};
use crate::ks::types::{prim, TypeDef, TypeId};
use crate::ks::value::Value;

use super::types_protocol::{eval, Flow, Protocol};
use super::Interpreter;

impl Interpreter {
    // ── Attr assignment: a.b = v ─────────────────────────────────────

    pub(super) fn exec_attr_assign(
        &mut self,
        object: &Spanned<Expr>,
        attr: &str,
        val: Value,
    ) -> Result<Flow, RuntimeError> {
        let Expr::Name(var_name) = &object.node else {
            return Err(ErrorKind::Unsupported("nested attr assignment not yet supported").into());
        };

        let slot = self.get_slot(var_name).ok_or_else(|| -> RuntimeError {
            ErrorKind::Undefined {
                kind: NameKind::Variable,
                name: var_name.clone(),
            }
            .into()
        })?;

        // Resolve type / field info from a snapshot of the current value.
        let (type_id, expected_tid) = {
            let current = slot.get();
            let Value::Rec { type_id, .. } = current else {
                return Err(ErrorKind::NoAttr {
                    type_id: current.type_id(),
                    attr: attr.to_string(),
                    access: AccessKind::Field,
                }
                .into());
            };
            let struct_fields = self
                .types
                .get_struct_fields(type_id)
                .map_err(RuntimeError::from)?;
            let expected_tid =
                struct_fields
                    .get(attr)
                    .copied()
                    .ok_or_else(|| -> RuntimeError {
                        ErrorKind::NoAttr {
                            type_id,
                            attr: attr.to_string(),
                            access: AccessKind::Field,
                        }
                        .into()
                    })?;
            (type_id, expected_tid)
        };

        let val = self.check_type(val, expected_tid)?;
        let field_idx = self.types.field_index(type_id, attr);

        // Mutate the Rec's field in place through the shared slot.
        slot.with_mut(|entry| {
            if let Value::Rec {
                type_id: tid,
                fields,
            } = entry
            {
                debug_assert_eq!(*tid, type_id);
                if let Some(idx) = field_idx {
                    Arc::make_mut(fields)[idx] = val;
                }
            }
        });
        Ok(Flow::Next(Value::Nil))
    }

    pub(super) fn exec_item_assign(
        &mut self,
        object: &Spanned<Expr>,
        args: &[Spanned<Expr>],
        val: Value,
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        let Expr::Name(var_name) = &object.node else {
            return Err(ErrorKind::Unsupported("nested index assignment not yet supported").into());
        };

        // Build args: [key_args..., val]
        let mut call_args = Vec::with_capacity(args.len() + 1);
        for a in args {
            call_args.push(eval!(self, a, out));
        }
        call_args.push(val);

        let receiver = self
            .get(var_name)
            .ok_or_else(|| -> RuntimeError {
                ErrorKind::Undefined {
                    kind: NameKind::Variable,
                    name: var_name.clone(),
                }
                .into()
            })?
            .clone();

        let set_item_id = self.protocol_methods.set_item;
        let set_item_name = Protocol::SetItem.method_name();
        self.call_method_by_id(&receiver, set_item_id, &call_args, out)
            .map_err(|e| {
                if matches!(e.kind, ErrorKind::NoAttr { ref attr, .. } if attr == set_item_name) {
                    RuntimeError::from(ErrorKind::NotIndexable {
                        type_id: receiver.type_id(),
                    })
                } else {
                    // Strip internal spans so the user's expression span wins.
                    RuntimeError {
                        span: None,
                        labels: Vec::new(),
                        ..e
                    }
                }
            })?;

        // Copy-out: write mutated self back to the variable in scope.
        if let Some(mutated_self) = self.last_method_self.take() {
            let _ = self.update_in_scope(var_name, mutated_self)?;
        }

        Ok(Flow::Next(Value::Nil))
    }

    // ── Attr: a.b ─────────────────────────────────────────────────────

    pub(super) fn eval_attr(&self, object: &Value, name: &str) -> Result<Flow, RuntimeError> {
        match object {
            Value::Type(type_id) => {
                let def = self.types.get(*type_id);
                match def {
                    TypeDef::EnumInstance { variants, .. } => {
                        // Try variant first, then static method.
                        if let Some((idx, _, vdef)) = variants.get_full(name) {
                            let variant_idx = idx as u32;
                            if vdef.fields.is_empty() {
                                return Ok(Flow::Next(Value::Enum {
                                    type_id: *type_id,
                                    variant_idx,
                                    fields: Arc::from(vec![]),
                                }));
                            } else {
                                return Ok(Flow::Next(Value::VariantConstructor {
                                    type_id: *type_id,
                                    variant_idx,
                                    field_types: vdef.fields.clone(),
                                }));
                            }
                        }
                        if let Some(method) = self.lookup_method(*type_id, name) {
                            return Ok(Flow::Next(method));
                        }
                        Err(ErrorKind::NoAttr {
                            type_id: *type_id,
                            attr: name.to_string(),
                            access: AccessKind::Variant,
                        }
                        .into())
                    }
                    _ => {
                        // Try static method lookup on this type.
                        // Bind as BoundMethod with Type receiver so the call
                        // path can resolve generic type parameters.
                        if let Some(method) = self.lookup_method(*type_id, name) {
                            return Ok(Flow::Next(Value::BoundMethod {
                                receiver: Box::new(Value::Type(*type_id)),
                                func: Box::new(method),
                            }));
                        }
                        Err(ErrorKind::NoAttr {
                            type_id: *type_id,
                            attr: name.to_string(),
                            access: AccessKind::Attr,
                        }
                        .into())
                    }
                }
            }
            Value::Rec { type_id, fields } => {
                if let Some(idx) = self.types.field_index(*type_id, name) {
                    if idx < fields.len() {
                        return Ok(Flow::Next(fields[idx].clone()));
                    }
                }
                if let Ok(bound) = self.resolve_method(object, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(ErrorKind::NoAttr {
                    type_id: *type_id,
                    attr: name.to_string(),
                    access: AccessKind::FieldOrMethod,
                }
                .into())
            }
            Value::Tup { type_id, fields: _ } => {
                // Positional access uses Expr::TupIdx (e.g., `t.0`), not name
                // lookup — the parser routes integer suffixes to TupIdx, so
                // we only land here for method calls like `t.len()`.
                if let Ok(bound) = self.resolve_method(object, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(ErrorKind::NoAttr {
                    type_id: *type_id,
                    attr: name.to_string(),
                    access: AccessKind::FieldOrMethod,
                }
                .into())
            }
            Value::Module(module_id) => {
                let module = self.native_registry.get_module(*module_id);
                match module.entries.get(name) {
                    Some(val) => Ok(Flow::Next(val.clone())),
                    None => Err(ErrorKind::NoAttr {
                        type_id: prim::NIL,
                        attr: name.to_string(),
                        access: AccessKind::Attr,
                    }
                    .into()),
                }
            }
            Value::AsType {
                inner,
                interface_id,
            } => {
                // Dispatch methods to the concrete inner type.
                if let Ok(bound) = self.resolve_method(inner, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(ErrorKind::NoAttr {
                    type_id: *interface_id,
                    attr: name.to_string(),
                    access: AccessKind::Method,
                }
                .into())
            }
            other => {
                if let Ok(bound) = self.resolve_method(other, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(ErrorKind::NoAttr {
                    type_id: other.type_id(),
                    attr: name.to_string(),
                    access: AccessKind::Attr,
                }
                .into())
            }
        }
    }

    // ── Item: a[b] ───────────────────────────────────────────────────────

    pub(super) fn eval_item(
        &mut self,
        object: &Value,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        match object {
            Value::Type(base_id) => {
                let type_args: Vec<TypeId> = args
                    .iter()
                    .map(|v| match v {
                        Value::Type(tid) => Ok(*tid),
                        other => Err(RuntimeError::from(ErrorKind::ExpectedType {
                            actual: other.type_id(),
                        })),
                    })
                    .collect::<Result<_, _>>()?;
                let instance_id = self
                    .types
                    .instantiate_by_kind(*base_id, type_args)
                    .map_err(RuntimeError::from)?;
                Ok(Flow::Next(Value::Type(instance_id)))
            }
            other => {
                let get_item_id = self.protocol_methods.get_item;
                let get_item_name = Protocol::GetItem.method_name();
                match self.call_method_by_id(other, get_item_id, args, out) {
                    Ok(val) => Ok(Flow::Next(val)),
                    Err(e) => {
                        // Convert "no method 'get_item'" → "not indexable".
                        if matches!(e.kind, ErrorKind::NoAttr { ref attr, .. } if attr == get_item_name)
                        {
                            Err(RuntimeError::from(ErrorKind::NotIndexable {
                                type_id: other.type_id(),
                            }))
                        } else {
                            // Strip internal spans so the user's expression span wins.
                            Err(RuntimeError {
                                span: None,
                                labels: Vec::new(),
                                ..e
                            })
                        }
                    }
                }
            }
        }
    }
}
