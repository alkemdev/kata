//! Expression evaluation: the giant `eval_expr` match plus the helpers
//! that only it needs (`eval_arr_lit`).
//!
//! Most variants are routed straight through to a sibling module:
//! `Match` → `eval_match` (match_.rs), `Call` → `eval_call` (call.rs),
//! `Attr` → `eval_attr` (access.rs), and so on. The work `eval_expr`
//! itself does is mostly *flow control* — evaluating sub-expressions in
//! the right order, handling early returns from `?` / `bail` / `cont` /
//! `ret`, attaching spans to errors that bubble up.

use std::io::Write;
use std::sync::Arc;

use indexmap::IndexMap;
use num_bigint::BigInt;
use tracing::trace;

use crate::ks::ast::{BinInterpPart, Expr, InterpPart, Span, Spanned};
use crate::ks::error::{
    AccessKind, ErrorKind, NameKind, RuntimeError, TypeKindExpectation,
};
use crate::ks::native::{self, NativeCtx};
use crate::ks::types::TypeId;
use crate::ks::value::Value;

use super::types_protocol::{eval, parse_int_literal, Flow};
use super::Interpreter;

impl Interpreter {
    /// Build an Arr[T] from a literal `[expr, expr, ...]`.
    /// Infers element type from the first element, type-checks the rest.
    fn eval_arr_lit(
        &mut self,
        elements: &[Spanned<Expr>],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        if elements.is_empty() {
            return Err(RuntimeError::new(ErrorKind::EmptyArrayLiteral)
                .help("array literals need at least one element to infer the type"));
        }

        // Evaluate all elements.
        let mut vals = Vec::with_capacity(elements.len());
        for elem in elements {
            vals.push(eval!(self, elem, out));
        }

        // Infer element type from the first element.
        let elem_tid = vals[0].type_id();

        // Type-check remaining elements — point to the offending element.
        for (i, val) in vals.iter().enumerate().skip(1) {
            let tid = val.type_id();
            if tid != elem_tid {
                return Err(RuntimeError::new(ErrorKind::TypeMismatch {
                    expected: elem_tid,
                    actual: tid,
                })
                .at(elements[i].span)
                .label(
                    elements[0].span,
                    format!(
                        "expected {} (inferred from first element)",
                        self.types.display_name(elem_tid)
                    ),
                )
                .label(elements[i].span, self.types.display_name(tid)));
            }
        }

        // Build the Arr[T] value via the shared NativeCtx helper. Type
        // checks are already done above; the helper trusts uniform types.
        let mut ctx = NativeCtx {
            types: &mut self.types,
            allocations: &mut self.allocations,
            intern: &mut self.intern,
            out,
            in_unsafe: self.in_unsafe,
        };
        let arr_val = ctx.build_arr(elem_tid, vals)?;
        Ok(Flow::Next(arr_val))
    }

    pub(super) fn eval_expr(
        &mut self,
        expr: &Spanned<Expr>,
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        trace!(?expr.node, "eval_expr");
        match &expr.node {
            Expr::Nil => Ok(Flow::Next(Value::Nil)),
            Expr::Bool(b) => Ok(Flow::Next(Value::Bool(*b))),
            Expr::Int(s) => {
                let n: BigInt = parse_int_literal(s).map_err(|reason| {
                    RuntimeError::new(ErrorKind::InvalidLiteral {
                        kind: "integer",
                        text: s.clone(),
                        reason,
                    })
                    .at(expr.span)
                })?;
                Ok(Flow::Next(Value::int(n)))
            }
            Expr::Float(s) => {
                let n: f64 = s.parse().map_err(|e: std::num::ParseFloatError| {
                    RuntimeError::new(ErrorKind::InvalidLiteral {
                        kind: "float",
                        text: s.clone(),
                        reason: e.to_string(),
                    })
                    .at(expr.span)
                })?;
                Ok(Flow::Next(Value::Float(n)))
            }
            Expr::Str(s) => Ok(Flow::Next(Value::Str(self.intern.intern_str(s)))),

            Expr::Interp { parts } => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        InterpPart::Lit(s) => result.push_str(s),
                        InterpPart::Expr(inner) => {
                            let flow = self.eval_expr(inner, out)?;
                            let val = match flow {
                                Flow::Next(v) => v,
                                flow @ (Flow::Return { .. }
                                | Flow::Propagate { .. }
                                | Flow::Bail(_)
                                | Flow::Cont(_)) => return Ok(flow),
                            };
                            result.push_str(&val.display(&self.types));
                        }
                    }
                }
                Ok(Flow::Next(Value::Str(
                    crate::ks::intern::InternTables::make_str(result),
                )))
            }

            Expr::BinLit(bytes) => Ok(Flow::Next(self.intern_bin(bytes.clone()))),

            Expr::BinInterp { parts } => {
                let mut result: Vec<u8> = Vec::new();
                for part in parts {
                    match part {
                        BinInterpPart::Bytes(bs) => result.extend_from_slice(bs),
                        BinInterpPart::Expr(inner) => {
                            let flow = self.eval_expr(inner, out)?;
                            let val = match flow {
                                Flow::Next(v) => v,
                                flow @ (Flow::Return { .. }
                                | Flow::Propagate { .. }
                                | Flow::Bail(_)
                                | Flow::Cont(_)) => return Ok(flow),
                            };
                            result.extend_from_slice(val.display(&self.types).as_bytes());
                        }
                    }
                }
                Ok(Flow::Next(self.intern_bin(result)))
            }

            Expr::Name(name) => self.get(name).map(Flow::Next).ok_or_else(|| {
                RuntimeError::new(ErrorKind::Undefined {
                    kind: NameKind::Variable,
                    name: name.clone(),
                })
                .at(expr.span)
                .help(format!("check the spelling, or define '{name}' with 'let'"))
            }),

            Expr::With { bindings, body } => {
                self.push_scope();
                for (name, val_expr) in bindings {
                    let val = eval!(self, val_expr, out);
                    self.set(name.clone(), val);
                }
                let result = self.exec_block(body, out);
                self.pop_scope(out);
                result
            }

            Expr::Unsafe { body } => {
                let was_unsafe = self.in_unsafe;
                self.in_unsafe = true;
                let result = self.exec_block(body, out);
                self.in_unsafe = was_unsafe;
                result
            }

            Expr::ArrLit { elements } => self
                .eval_arr_lit(elements, out)
                .map_err(|e: RuntimeError| e.at(expr.span)),

            Expr::TupLit { elements } => {
                let mut vals = Vec::with_capacity(elements.len());
                for elem in elements {
                    vals.push(eval!(self, elem, out));
                }
                let type_args: Vec<TypeId> = vals.iter().map(|v| v.type_id()).collect();
                let tup_tid = self.types.instantiate_tup(type_args);
                Ok(Flow::Next(Value::Tup {
                    type_id: tup_tid,
                    fields: Arc::from(vals),
                }))
            }

            Expr::Match {
                keyword,
                subject,
                arms,
            } => self
                .eval_match(*keyword, subject, arms, out)
                .map_err(|e: RuntimeError| e.at(expr.span)),

            Expr::Ques(inner) => {
                let val = eval!(self, inner, out);
                if let Value::Enum {
                    type_id,
                    variant_idx,
                    ref fields,
                    ..
                } = val
                {
                    if let Some(shape) = self.types.try_shape(type_id) {
                        if variant_idx == shape.val_idx() {
                            if let Some(inner_val) = fields.first() {
                                return Ok(Flow::Next(inner_val.clone()));
                            }
                        } else {
                            // Non (Opt) or Err (Res) — propagate upward.
                            return Ok(Flow::Propagate {
                                value: val,
                                span: expr.span,
                            });
                        }
                    }
                }
                Err(RuntimeError::new(ErrorKind::InvalidUnwrap { operator: "?" }).at(expr.span))
            }

            Expr::Bang(inner) => {
                let val = eval!(self, inner, out);
                if let Value::Enum {
                    type_id,
                    variant_idx,
                    ref fields,
                    ..
                } = val
                {
                    if let Some(shape) = self.types.try_shape(type_id) {
                        if variant_idx == shape.val_idx() {
                            if let Some(inner_val) = fields.first() {
                                return Ok(Flow::Next(inner_val.clone()));
                            }
                        } else {
                            // Non (Opt) or Err (Res) — panic with a message
                            // formatted from the variant name. The lookup
                            // here is one hash hit and only fires on the
                            // failure path, not the hot path.
                            let variant = self.types.variant_name(type_id, variant_idx).to_string();
                            return Err(RuntimeError::new(ErrorKind::UnwrapFailed {
                                type_id,
                                variant,
                            })
                            .at(expr.span));
                        }
                    }
                }
                Err(RuntimeError::new(ErrorKind::InvalidUnwrap { operator: "!" }).at(expr.span))
            }

            Expr::As { value, target } => {
                let val = eval!(self, value, out);
                let target_id = self.resolve_type_expr(&target.node)?;
                let actual_id = val.type_id();
                if actual_id == target_id {
                    Ok(Flow::Next(val))
                } else if self.conforms_to(actual_id, target_id) {
                    Ok(Flow::Next(Value::AsType {
                        inner: Box::new(val),
                        interface_id: target_id,
                    }))
                } else {
                    Err(RuntimeError::new(ErrorKind::AsNonConforming {
                        actual: actual_id,
                        interface: target_id,
                    })
                    .at(expr.span))
                }
            }

            Expr::Attr {
                object,
                name,
                name_span,
            } => {
                let obj = eval!(self, object, out);
                self.eval_attr(&obj, name)
                    .map_err(|e: RuntimeError| e.at(*name_span))
            }

            Expr::TupIdx {
                object,
                idx,
                idx_span,
            } => {
                let obj = eval!(self, object, out);
                let Value::Tup { fields, type_id } = &obj else {
                    return Err(RuntimeError::new(ErrorKind::TupIdxOnNonTuple {
                        type_id: obj.type_id(),
                        idx: *idx,
                    })
                    .at(*idx_span));
                };
                let i = *idx as usize;
                if i >= fields.len() {
                    return Err(RuntimeError::new(ErrorKind::PrimOutOfRange {
                        type_name: "Tup",
                        detail: format!(
                            "index {i}, length {} for {}",
                            fields.len(),
                            self.types.display_name(*type_id),
                        ),
                    })
                    .at(*idx_span));
                }
                Ok(Flow::Next(fields[i].clone()))
            }

            Expr::Item { object, args } => {
                let obj = eval!(self, object, out);
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(eval!(self, a, out));
                }

                // Type instantiation — no copy-out needed.
                if matches!(obj, Value::Type(_)) {
                    return self
                        .eval_item(&obj, &arg_vals, out)
                        .map_err(|e: RuntimeError| e.at(expr.span));
                }

                // Indexing: extract receiver variable name for copy-out.
                let receiver_var = if let Expr::Name(var) = &object.node {
                    Some(var.clone())
                } else {
                    None
                };

                let result = self
                    .eval_item(&obj, &arg_vals, out)
                    .map_err(|e: RuntimeError| e.at(expr.span))?;

                // Copy-out: write mutated self back to receiver variable.
                if let Some(var_name) = receiver_var {
                    if let Some(mutated_self) = self.last_method_self.take() {
                        // If the variable holds an AsType, re-wrap after copy-out.
                        let write_back =
                            if let Some(Value::AsType { interface_id, .. }) = self.get(&var_name) {
                                let iface = interface_id;
                                Value::AsType {
                                    inner: Box::new(mutated_self),
                                    interface_id: iface,
                                }
                            } else {
                                mutated_self
                            };
                        let _ = self
                            .update_in_scope(&var_name, write_back)
                            .map_err(|e: RuntimeError| e.at(expr.span))?;
                    }
                }

                Ok(result)
            }

            Expr::Call {
                callee,
                args,
                args_span,
            } => {
                // Evaluate args once, keeping spans for error reporting.
                let mut arg_vals = Vec::with_capacity(args.len());
                let arg_spans: Vec<Span> = args.iter().map(|a| a.span).collect();
                for a in args {
                    arg_vals.push(eval!(self, a, out));
                }

                let func = eval!(self, callee, out);

                // Method call with copy-in copy-out: if callee was obj.method,
                // extract the receiver variable name for self write-back.
                let receiver_var = if matches!(func, Value::BoundMethod { .. }) {
                    if let Expr::Attr { object, .. } = &callee.node {
                        if let Expr::Name(var) = &object.node {
                            Some(var.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let result = self.eval_call(func, &arg_vals, &arg_spans, out).map_err(
                    |e: RuntimeError| {
                        // Arity errors point to args list; type errors already have spans.
                        if matches!(e.kind, ErrorKind::ArityMismatch { .. }) {
                            e.at(*args_span)
                        } else {
                            e.at(expr.span)
                        }
                    },
                )?;

                // Copy-in copy-out: write mutated self back to the receiver variable.
                if let Some(var_name) = receiver_var {
                    if let Some(mutated_self) = self.last_method_self.take() {
                        let write_back =
                            if let Some(Value::AsType { interface_id, .. }) = self.get(&var_name) {
                                let iface = interface_id;
                                Value::AsType {
                                    inner: Box::new(mutated_self),
                                    interface_id: iface,
                                }
                            } else {
                                mutated_self
                            };
                        let _ = self
                            .update_in_scope(&var_name, write_back)
                            .map_err(|e: RuntimeError| e.at(expr.span))?;
                    }
                }

                Ok(result)
            }

            Expr::BinOp { op, left, right } => {
                let lv = eval!(self, left, out);
                let rv = eval!(self, right, out);
                let result = native::eval_binop(*op, &lv, &rv).map_err(|e| {
                    let mut err = RuntimeError::from(e);
                    if matches!(err.kind, ErrorKind::BinOpType { .. }) {
                        let left_type = self.types.display_name(lv.type_id());
                        let right_type = self.types.display_name(rv.type_id());
                        err = err
                            .at(left.span)
                            .label(left.span, left_type.clone())
                            .label(right.span, right_type.clone());
                        if left_type != right_type {
                            err = err.help(format!(
                                "'{sym}' works on matching types: Int{sym}Int, Float{sym}Float, or Str{sym}Str",
                                sym = op.symbol()
                            ));
                        }
                    } else {
                        err = err.at(expr.span);
                    }
                    err
                })?;
                Ok(Flow::Next(result))
            }

            Expr::If {
                cond,
                then_body,
                else_body,
            } => {
                let cv = eval!(self, cond, out);
                if native::truth(&cv) {
                    self.push_scope();
                    let result = self.exec_block(then_body, out);
                    self.pop_scope(out);
                    result
                } else if let Some(else_stmts) = else_body {
                    self.push_scope();
                    let result = self.exec_block(else_stmts, out);
                    self.pop_scope(out);
                    result
                } else {
                    Ok(Flow::Next(Value::Nil))
                }
            }

            Expr::For {
                pattern,
                iter_expr,
                body,
            } => {
                self.check_unique_bindings(pattern)?;
                let iterable = eval!(self, iter_expr, out);
                let to_iter_id = self.protocol_methods.to_iter;
                let next_id = self.protocol_methods.next;
                let iter_val = self
                    .call_method_by_id(&iterable, to_iter_id, &[], out)
                    .map_err(|e: RuntimeError| e.at(expr.span))?;

                let mut iterator = iter_val;
                loop {
                    let bound = self
                        .resolve_method_by_id(&iterator, next_id)
                        .map_err(|e: RuntimeError| e.at(expr.span))?;
                    let next_result = match self
                        .eval_call(bound, &[], &[], out)
                        .map_err(|e: RuntimeError| e.at(expr.span))?
                    {
                        Flow::Next(v) => v,
                        _ => break,
                    };

                    if let Some(mutated_self) = self.last_method_self.take() {
                        iterator = mutated_self;
                    }

                    let Value::Enum {
                        type_id: opt_tid,
                        variant_idx,
                        fields,
                    } = &next_result
                    else {
                        return Err(RuntimeError::new(ErrorKind::IteratorProtocol(
                            "iterator .next() must return an Opt value",
                        ))
                        .at(expr.span));
                    };

                    // The .next() return type must be Opt-shaped; compare by
                    // variant index rather than name so the hot loop path is
                    // a u32 compare, not a string lookup.
                    let Some(crate::ks::types::TryShape::OptLike { none_idx, .. }) =
                        self.types.try_shape(*opt_tid)
                    else {
                        return Err(RuntimeError::new(ErrorKind::IteratorProtocol(
                            "iterator .next() must return an Opt value",
                        ))
                        .at(expr.span));
                    };
                    if *variant_idx == none_idx {
                        break;
                    }

                    let val = fields.first().cloned().ok_or_else(|| {
                        RuntimeError::new(ErrorKind::IteratorProtocol("Opt.Some has no field"))
                            .at(expr.span)
                    })?;

                    self.push_scope();
                    let bindings = self
                        .destructure_irrefutable(pattern, &val)
                        .map_err(|e| e.at(pattern.span))?;
                    for (name, v) in bindings {
                        self.set(name, v);
                    }

                    let flow = self.exec_block(body, out)?;
                    if let Some(early) = self.dispatch_loop_flow(flow, out) {
                        return Ok(early);
                    }
                }

                Ok(Flow::Next(Value::Nil))
            }

            Expr::While { cond, body } => {
                loop {
                    let cv = eval!(self, cond, out);
                    if !native::truth(&cv) {
                        break;
                    }
                    self.push_scope();
                    let flow = self.exec_block(body, out)?;
                    if let Some(early) = self.dispatch_loop_flow(flow, out) {
                        return Ok(early);
                    }
                }
                Ok(Flow::Next(Value::Nil))
            }

            Expr::And { left, right } => {
                let lv = eval!(self, left, out);
                if !native::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = eval!(self, right, out);
                Ok(Flow::Next(rv))
            }

            Expr::Or { left, right } => {
                let lv = eval!(self, left, out);
                if native::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = eval!(self, right, out);
                Ok(Flow::Next(rv))
            }

            Expr::UnaryOp { op, operand } => {
                let val = eval!(self, operand, out);
                let result = native::eval_unaryop(*op, &val)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(result))
            }

            Expr::Construct {
                type_expr,
                fields,
                open_brace,
            } => {
                let type_val = eval!(self, type_expr, out);
                let Value::Type(type_id) = type_val else {
                    return Err(RuntimeError::new(ErrorKind::WrongTypeKind {
                        type_id: type_val.type_id(),
                        expected: TypeKindExpectation::Constructible,
                    })
                    .at(expr.span));
                };

                let expected_fields = self
                    .types
                    .get_struct_fields(type_id)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?
                    .clone();

                for (fname, _) in fields {
                    if !expected_fields.contains_key(fname.as_str()) {
                        return Err(RuntimeError::new(ErrorKind::NoAttr {
                            type_id,
                            attr: fname.clone(),
                            access: AccessKind::Field,
                        })
                        .at(expr.span));
                    }
                }

                let mut provided: IndexMap<String, (Value, Span)> = IndexMap::new();
                for (fname, fexpr) in fields {
                    let val = eval!(self, fexpr, out);
                    provided.insert(fname.clone(), (val, fexpr.span));
                }

                let mut result_fields = Vec::with_capacity(expected_fields.len());
                for (fname, expected_tid) in &expected_fields {
                    let (val, val_span) =
                        provided.shift_remove(fname.as_str()).ok_or_else(|| {
                            RuntimeError::new(ErrorKind::MissingField {
                                type_id,
                                field: fname.clone(),
                            })
                            .at((type_expr.span.0, open_brace.1))
                        })?;
                    let checked_val = self
                        .check_type(val, *expected_tid)
                        .map_err(|e| e.at(val_span))?;
                    result_fields.push(checked_val);
                }

                Ok(Flow::Next(Value::Rec {
                    type_id,
                    fields: Arc::from(result_fields),
                }))
            }
        }
    }
}
