//! Call-site dispatch: method lookup, prim-type constructors, function
//! invocation, and the shared `call_func_body` that handles param
//! type-checking, scope context-switching, generic type-param binding,
//! and copy-out for `self`.

use std::io::Write;
use std::sync::Arc;

use crate::ks::ast::{Span, Spanned, Stmt};
use crate::ks::error::{
    AccessKind, ArityTarget, ErrorKind, FlowMisuse, RuntimeError, TypeKindExpectation,
};
use crate::ks::native::NativeCtx;
use crate::ks::types::{prim, TypeExpr, TypeId};
use crate::ks::value::{FuncParam, Value};

use super::types_protocol::{Flow, SELF_PARAM};
use super::Interpreter;

impl Interpreter {
    // ── Method helpers ────────────────────────────────────────────────

    /// Look up a method by name. Falls back from instance to base type
    /// (e.g., Buf[Int] → Buf) so generic methods work.
    ///
    /// Resolves the name to a `MethodId` via the interner; if the name
    /// was never registered as a method anywhere, the lookup short-
    /// circuits to `None` without touching the methods table.
    pub(super) fn lookup_method(&self, type_id: TypeId, name: &str) -> Option<Value> {
        let method_id = self.method_interner.lookup(name)?;
        // Try exact type first.
        if let Some(method) = self.methods.get(&type_id).and_then(|t| t.get(&method_id)) {
            return Some(method.clone());
        }
        // Fall back to base type for instances.
        let base = self.types.base_type(type_id);
        if base != type_id {
            return self
                .methods
                .get(&base)
                .and_then(|t| t.get(&method_id))
                .cloned();
        }
        None
    }

    /// Wrap a Func value as a BoundMethod with the given receiver.
    pub(super) fn bind_method(
        &self,
        receiver: Value,
        method: Value,
        _name: &str,
    ) -> Result<Value, RuntimeError> {
        if !matches!(method, Value::Func(_) | Value::NativeFn(_)) {
            return Err(
                ErrorKind::InternalError("bound method must wrap a Func or NativeFn").into(),
            );
        }
        Ok(Value::BoundMethod {
            receiver: Box::new(receiver),
            func: Box::new(method),
        })
    }

    /// Look up and bind a method, ready to call.
    pub(super) fn resolve_method(
        &self,
        receiver: &Value,
        name: &str,
    ) -> Result<Value, RuntimeError> {
        let tid = receiver.type_id();
        let func = self
            .lookup_method(tid, name)
            .ok_or_else(|| -> RuntimeError {
                ErrorKind::NoAttr {
                    type_id: tid,
                    attr: name.to_string(),
                    access: AccessKind::Method,
                }
                .into()
            })?;
        self.bind_method(receiver.clone(), func, name)
    }

    /// Call a method on a value by name (no copy-out — caller handles that).
    pub(super) fn call_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        let bound = self.resolve_method(receiver, name)?;
        match self.eval_call(bound, args, &[], out)? {
            Flow::Next(v) | Flow::Return { value: v, .. } | Flow::Propagate { value: v, .. } => {
                Ok(v)
            }
            _ => Err(ErrorKind::InternalError("method returned abnormal flow").into()),
        }
    }

    // ── Call: a(b) ───────────────────────────────────────────────────────

    pub(super) fn eval_call(
        &mut self,
        func: Value,
        args: &[Value],
        arg_spans: &[Span],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        match func {
            Value::Func(f) => {
                if args.len() != f.params.len() {
                    return Err(ErrorKind::ArityMismatch {
                        target: ArityTarget::Function,
                        expected: f.params.len(),
                        actual: args.len(),
                    }
                    .into());
                }

                let result = self.call_func_body(
                    &f.params,
                    args,
                    arg_spans,
                    &f.ret_type,
                    &f.body,
                    false,
                    &[],
                    &f.closure_scope,
                    out,
                )?;
                Ok(Flow::Next(result))
            }

            Value::VariantConstructor {
                type_id,
                variant_idx,
                field_types,
            } => {
                if args.len() != field_types.len() {
                    let variant_name = self.types.variant_name(type_id, variant_idx);
                    return Err(ErrorKind::ArityMismatch {
                        target: ArityTarget::Variant {
                            name: variant_name.to_string(),
                        },
                        expected: field_types.len(),
                        actual: args.len(),
                    }
                    .into());
                }

                let mut checked_fields = Vec::with_capacity(args.len());
                for (i, (val, &expected)) in args.iter().zip(field_types.iter()).enumerate() {
                    let checked = self.check_type(val.clone(), expected).map_err(|e| {
                        if let Some(&span) = arg_spans.get(i) {
                            e.at(span)
                        } else {
                            e
                        }
                    })?;
                    checked_fields.push(checked);
                }

                Ok(Flow::Next(Value::Enum {
                    type_id,
                    variant_idx,
                    fields: Arc::from(checked_fields),
                }))
            }

            Value::BoundMethod { receiver, func } => match *func {
                Value::Func(f) => {
                    // Static method call: receiver is Value::Type(tid).
                    // Don't prepend self, just pass type args for generic resolution.
                    if let Value::Type(tid) = *receiver {
                        if args.len() != f.params.len() {
                            return Err(ErrorKind::ArityMismatch {
                                target: ArityTarget::Function,
                                expected: f.params.len(),
                                actual: args.len(),
                            }
                            .into());
                        }
                        // Resolve generic type params: Map[Str, Int] → K=Str, V=Int
                        // Augment the closure scope with type param bindings.
                        let type_args = self.types.instance_type_args(tid);
                        let base_id = self.types.base_type(tid);
                        let param_names = self.types.type_param_names(base_id);
                        let augmented_scope = {
                            let mut type_frame = crate::ks::scope::Frame::new();
                            for (name, &ta) in param_names.iter().zip(type_args.iter()) {
                                type_frame.set(name.clone(), Value::Type(ta));
                            }
                            Some(Arc::new(crate::ks::scope::Scope {
                                frame: type_frame,
                                parent: f.closure_scope.clone(),
                            }))
                        };

                        let result = self.call_func_body(
                            &f.params,
                            args,
                            arg_spans,
                            &f.ret_type,
                            &f.body,
                            false,
                            &type_args,
                            &augmented_scope,
                            out,
                        )?;
                        return Ok(Flow::Next(result));
                    }

                    // Instance method call: prepend receiver as self.
                    let method_params = &f.params[1..];
                    if args.len() != method_params.len() {
                        return Err(ErrorKind::ArityMismatch {
                            target: ArityTarget::Method,
                            expected: method_params.len(),
                            actual: args.len(),
                        }
                        .into());
                    }

                    let mut full_args = Vec::with_capacity(f.params.len());
                    let receiver_type_args = self.types.instance_type_args(receiver.type_id());
                    full_args.push(*receiver);
                    full_args.extend_from_slice(args);

                    let mut full_spans = Vec::with_capacity(f.params.len());
                    full_spans.push((0, 0));
                    full_spans.extend_from_slice(arg_spans);

                    let result = self.call_func_body(
                        &f.params,
                        &full_args,
                        &full_spans,
                        &f.ret_type,
                        &f.body,
                        true,
                        &receiver_type_args,
                        &f.closure_scope,
                        out,
                    )?;
                    Ok(Flow::Next(result))
                }
                Value::NativeFn(fn_id) => {
                    let entry = self.native_registry.get(fn_id);
                    let handler = entry.handler;
                    // Static native method (Type receiver): don't prepend self.
                    // Instance native method: prepend receiver as first arg.
                    let full_args = if matches!(*receiver, Value::Type(_)) {
                        args.to_vec()
                    } else {
                        let mut fa = Vec::with_capacity(args.len() + 1);
                        fa.push(*receiver);
                        fa.extend_from_slice(args);
                        fa
                    };
                    let mut ctx = NativeCtx {
                        types: &mut self.types,
                        allocations: &mut self.allocations,
                        intern: &mut self.intern,
                        out,
                        in_unsafe: self.in_unsafe,
                    };
                    let result = handler(&mut ctx, &full_args)?;
                    Ok(Flow::Next(result))
                }
                _ => Err(ErrorKind::InternalError("bound method wraps unexpected value").into()),
            },

            Value::NativeFn(fn_id) => {
                let entry = self.native_registry.get(fn_id);
                if entry.requires_unsafe && !self.in_unsafe {
                    return Err(RuntimeError::new(ErrorKind::UnsafeRequired {
                        intrinsic: entry.name.to_string(),
                    })
                    .help("wrap the call in an unsafe { ... } block"));
                }
                let handler = entry.handler;
                let mut ctx = NativeCtx {
                    types: &mut self.types,
                    allocations: &mut self.allocations,
                    intern: &mut self.intern,
                    out,
                    in_unsafe: self.in_unsafe,
                };
                let result = handler(&mut ctx, args)?;
                Ok(Flow::Next(result))
            }

            // Prim type constructor calls: Byte(255), Char(97), etc.
            Value::Type(tid) => self.eval_prim_constructor(tid, args),

            other => Err(ErrorKind::WrongTypeKind {
                type_id: other.type_id(),
                expected: TypeKindExpectation::Callable,
            }
            .into()),
        }
    }

    /// Handle prim type constructor calls: `Byte(255)`, `Char(97)`, etc.
    fn eval_prim_constructor(
        &self,
        type_id: TypeId,
        args: &[Value],
    ) -> Result<Flow, RuntimeError> {
        if args.len() != 1 {
            return Err(ErrorKind::ArityMismatch {
                target: ArityTarget::Function,
                expected: 1,
                actual: args.len(),
            }
            .into());
        }
        let arg = &args[0];
        match type_id {
            prim::BYTE => {
                let Value::Int(n) = arg else {
                    return Err(ErrorKind::TypeMismatch {
                        expected: prim::INT,
                        actual: arg.type_id(),
                    }
                    .into());
                };
                let val: i64 = n
                    .as_ref()
                    .try_into()
                    .map_err(|_| ErrorKind::IntegerOverflow)?;
                if !(0..=255).contains(&val) {
                    return Err(ErrorKind::PrimOutOfRange {
                        type_name: "Byte",
                        detail: format!("{val} (must be 0-255)"),
                    }
                    .into());
                }
                Ok(Flow::Next(Value::Byte(val as u8)))
            }
            prim::CHAR => {
                let Value::Int(n) = arg else {
                    return Err(ErrorKind::TypeMismatch {
                        expected: prim::INT,
                        actual: arg.type_id(),
                    }
                    .into());
                };
                let val: u32 = n
                    .as_ref()
                    .try_into()
                    .map_err(|_| ErrorKind::IntegerOverflow)?;
                let ch = char::from_u32(val).ok_or_else(|| ErrorKind::PrimOutOfRange {
                    type_name: "Char",
                    detail: format!("invalid Unicode codepoint 0x{val:X}"),
                })?;
                Ok(Flow::Next(Value::Char(ch)))
            }
            _ => {
                if let Some(result) = crate::ks::numeric::try_construct(type_id, arg) {
                    return result.map(Flow::Next).map_err(|e| e.into());
                }
                Err(ErrorKind::WrongTypeKind {
                    type_id,
                    expected: TypeKindExpectation::Callable,
                }
                .into())
            }
        }
    }

    /// Run a function body. Type-checks args, switches scope context, binds
    /// the body's parameters and any generic type params (for methods on
    /// instances of generic types), executes the block, drops locals,
    /// restores the caller's scope, and type-checks the return value.
    pub(super) fn call_func_body(
        &mut self,
        params: &[FuncParam],
        args: &[Value],
        arg_spans: &[Span],
        ret_type: &Option<TypeExpr>,
        body: &[Spanned<Stmt>],
        is_method: bool,
        instance_type_args: &[TypeId],
        func_closure: &Option<Arc<crate::ks::scope::Scope>>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        // Type-check arguments before switching context.
        let mut checked_args: Vec<Value> = Vec::with_capacity(args.len());
        for (i, (param, val)) in params.iter().zip(args.iter()).enumerate() {
            let checked = if let Some(ref texpr) = param.type_ann {
                let expected = self
                    .types
                    .resolve_texpr(texpr.clone(), instance_type_args)?;
                self.check_type(val.clone(), expected).map_err(|e| {
                    if let Some(&span) = arg_spans.get(i) {
                        e.at(span)
                    } else {
                        e
                    }
                })?
            } else {
                val.clone()
            };
            checked_args.push(checked);
        }

        // Context switch: enter the callee's lexical scope.
        let saved_stack = std::mem::take(&mut self.call_stack);
        let saved_closure = self.closure_scope.take();
        self.closure_scope = func_closure.clone();
        self.call_stack = vec![crate::ks::scope::Frame::new()];
        for (param, val) in params.iter().zip(checked_args.iter()) {
            self.set(param.name.clone(), val.clone());
        }

        // Bind generic type params (T, K, V, etc.) in the callee's frame.
        if is_method && !instance_type_args.is_empty() {
            if let Some(receiver) = args.first() {
                let base_id = self.types.base_type(receiver.type_id());
                let type_param_names = self.types.type_param_names(base_id);
                for (name, &tid) in type_param_names.iter().zip(instance_type_args.iter()) {
                    self.set(name.clone(), Value::Type(tid));
                }
            }
        }

        let block_result = self.exec_block(body, out);

        // Copy-out: stash mutated self before cleanup.
        if is_method {
            self.last_method_self = self.get(SELF_PARAM).cloned();
            self.remove(SELF_PARAM);
        }

        // Drop locals in the callee frame.
        let callee_frame = self.call_stack.pop().unwrap_or_default();
        if !self.dropping {
            for (_name, value) in callee_frame.drain() {
                self.drop_value(value, out);
            }
        }

        // Restore caller context.
        self.call_stack = saved_stack;
        self.closure_scope = saved_closure;

        let mut result = match block_result {
            Ok(Flow::Next(v) | Flow::Return { value: v, .. } | Flow::Propagate { value: v, .. }) => {
                v
            }
            Ok(Flow::Bail(span)) => {
                return Err(
                    RuntimeError::new(ErrorKind::FlowMisuse(FlowMisuse::BailOutsideLoop)).at(span),
                );
            }
            Ok(Flow::Cont(span)) => {
                return Err(
                    RuntimeError::new(ErrorKind::FlowMisuse(FlowMisuse::ContOutsideLoop)).at(span),
                );
            }
            Err(e) => return Err(e),
        };

        if let Some(ref ret_texpr) = ret_type {
            let expected_ret = self
                .types
                .resolve_texpr(ret_texpr.clone(), instance_type_args)?;
            result = self.check_type(result, expected_ret)?;
        }

        Ok(result)
    }
}
