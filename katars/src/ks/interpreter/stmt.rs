//! Statement execution and program / block / REPL drivers.
//!
//! - `exec_stmt`: dispatches a single statement.
//! - `exec_block`: runs a block, collapsing the trailing `Flow::Next` value.
//! - `exec_program` / `exec_top_level`: top-level entry, rejects flow control
//!   that escapes a function or loop.
//! - `exec_repl`: same as `exec_top_level` but prints non-Nil expression
//!   values for an interactive session.
//! - `dispatch_loop_flow`: shared bail/cont/ret routing for loops.

use std::io::Write;
use std::sync::Arc;

use tracing::{debug, trace};

use crate::ks::ast::{AssignTarget, Expr, FuncDef, Param, Program, Spanned, Stmt};
use crate::ks::error::{ErrorKind, FlowMisuse, RuntimeError};
use crate::ks::types::TypeExpr;
use crate::ks::value::{FuncData, FuncParam, Value};

use super::types_protocol::{eval, Flow, INTERFACE_DROP};
use super::Interpreter;

impl Interpreter {
    // ── Program execution ────────────────────────────────────────────────

    /// Execute a program (prelude or user code).
    pub fn exec_program(
        &mut self,
        program: &Program,
        prelude: Option<&Program>,
        out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        if let Some(pre) = prelude {
            debug!(stmts = pre.len(), "loading prelude");
            self.exec_top_level(pre, out)?;
        }

        debug!(stmts = program.len(), "exec_program");
        self.exec_top_level(program, out)?;
        Ok(())
    }

    // ── Statement execution ──────────────────────────────────────────────

    pub(super) fn exec_stmt(
        &mut self,
        stmt: &Spanned<Stmt>,
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        trace!(?stmt.node, "exec_stmt");
        match &stmt.node {
            Stmt::Expr(expr) => self.eval_expr(expr, out),

            Stmt::EnumDef {
                name,
                type_params,
                variants,
            } => {
                self.register_enum(&name.node, type_params, variants)
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::KindDef {
                name,
                type_params,
                fields,
            } => {
                self.register_struct(&name.node, type_params, fields)
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::InterfaceDef {
                name,
                type_params,
                methods,
            } => {
                self.register_interface(&name.node, type_params, methods)
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Impl {
                target,
                as_type,
                methods,
            } => {
                let (type_id, bindings) = self
                    .resolve_type_pattern(&target.node)
                    .map_err(|e| e.at(target.span))?;
                self.register_impl_methods(type_id, &bindings, methods)
                    .map_err(|e| e.at(stmt.span))?;
                if let Some(iface_expr) = as_type {
                    self.check_conformance(type_id, &iface_expr.node)
                        .map_err(|e| e.at(iface_expr.span))?;
                    // Track lifecycle protocol implementations.
                    let iface_name = match &iface_expr.node {
                        Expr::Name(n) => Some(n.as_str()),
                        Expr::Item { object, .. } => {
                            if let Expr::Name(n) = &object.node {
                                Some(n.as_str())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(name) = iface_name {
                        let iface_id =
                            self.resolve_type(name).map_err(|e| e.at(iface_expr.span))?;
                        // Record conformance for dynamic dispatch.
                        let type_base = self.types.base_type(type_id);
                        let iface_base = self.types.base_type(iface_id);
                        self.conformances.insert((type_base, iface_base));
                        if name == INTERFACE_DROP {
                            self.drop_types.insert(type_id);
                        }
                    }
                }
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::FuncDef(FuncDef {
                name,
                params,
                ret_type,
                body,
            }) => {
                let func_params = self.resolve_params(params).map_err(|e| e.at(stmt.span))?;

                let ret_texpr = ret_type
                    .as_ref()
                    .map(|ann| -> Result<TypeExpr, RuntimeError> {
                        let tid = self
                            .resolve_type_expr(&ann.node)
                            .map_err(|e| e.at(ann.span))?;
                        Ok(TypeExpr::Concrete(tid))
                    })
                    .transpose()?;

                let captured = self.capture_scope();
                let func = Value::Func(Arc::new(FuncData {
                    params: func_params,
                    ret_type: ret_texpr,
                    body: body.clone(),
                    closure_scope: Some(captured),
                }));
                self.set(name.node.clone(), func);
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Let { pattern, value } => match self.eval_expr(value, out)? {
                Flow::Next(val) => {
                    self.check_unique_bindings(pattern)?;
                    let bindings = self.destructure_irrefutable(pattern, &val)?;
                    for (name, v) in bindings {
                        self.set(name, v);
                    }
                    Ok(Flow::Next(Value::Nil))
                }
                flow @ (Flow::Return { .. } | Flow::Propagate { .. }) => Ok(flow),
                _ => Ok(Flow::Next(Value::Nil)),
            },

            Stmt::Assign { target, value } => {
                let val = eval!(self, value, out);
                match target {
                    AssignTarget::Name(name) => {
                        let old = self
                            .update_in_scope(name, val)
                            .map_err(|e| e.at(stmt.span))?;
                        if let Some(old_val) = old {
                            self.drop_value(old_val, out);
                        }
                        Ok(Flow::Next(Value::Nil))
                    }
                    AssignTarget::Attr { object, attr } => self
                        .exec_attr_assign(object, attr, val)
                        .map_err(|e| e.at(stmt.span)),
                    AssignTarget::Item { object, args } => self
                        .exec_item_assign(object, args, val, out)
                        .map_err(|e| e.at(stmt.span)),
                }
            }

            Stmt::Import { path, names } => {
                self.exec_import(path, names.as_deref(), out)?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Bail { keyword } => Ok(Flow::Bail(*keyword)),
            Stmt::Cont { keyword } => Ok(Flow::Cont(*keyword)),

            Stmt::Ret { keyword, value } => {
                let val = eval!(self, value, out);
                Ok(Flow::Return {
                    value: val,
                    span: *keyword,
                })
            }
        }
    }

    // ── Block execution ──────────────────────────────────────────────────

    pub(super) fn exec_block(
        &mut self,
        stmts: &[Spanned<Stmt>],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        let mut last_val = Value::Nil;
        for stmt in stmts {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(v) => last_val = v,
                flow @ (Flow::Return { .. }
                | Flow::Propagate { .. }
                | Flow::Bail(_)
                | Flow::Cont(_)) => return Ok(flow),
            }
        }
        Ok(Flow::Next(last_val))
    }

    // ── Shared helpers ──────────────────────────────────────────────────

    /// Resolve AST params to FuncParam values (no generic type params).
    pub(super) fn resolve_params(
        &mut self,
        params: &[Param],
    ) -> Result<Vec<FuncParam>, RuntimeError> {
        self.resolve_params_with_type_params(params, &[])
    }

    /// Resolve AST params to FuncParam values, with optional generic type params.
    pub(super) fn resolve_params_with_type_params(
        &self,
        params: &[Param],
        type_params: &[String],
    ) -> Result<Vec<FuncParam>, RuntimeError> {
        params
            .iter()
            .map(|p| {
                let type_ann = p
                    .type_ann
                    .as_ref()
                    .map(|ann| self.resolve_type_ann(&ann.node, type_params))
                    .transpose()?;
                Ok(FuncParam {
                    name: p.name.node.clone(),
                    type_ann,
                })
            })
            .collect()
    }

    /// Execute a top-level statement list, rejecting ret/break/continue/?.
    fn exec_top_level(
        &mut self,
        stmts: &[Spanned<Stmt>],
        out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        for stmt in stmts {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(_) => {}
                Flow::Return { span, .. } => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::RetOutsideFunction,
                    ))
                    .at(span)
                    .note("ret can only be used inside a func body"));
                }
                Flow::Propagate { span, .. } => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::PropagateOutsideFunction,
                    ))
                    .at(span)
                    .note("? can only be used inside a func body"));
                }
                Flow::Bail(span) => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::BailOutsideLoop,
                    ))
                    .at(span)
                    .note("bail can only be used inside while or for loops"))
                }
                Flow::Cont(span) => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::ContOutsideLoop,
                    ))
                    .at(span)
                    .note("cont can only be used inside while or for loops"))
                }
            }
        }
        Ok(())
    }

    /// Execute statements in REPL mode: expression results are printed.
    /// Non-Nil values from expression statements get displayed.
    pub fn exec_repl(
        &mut self,
        stmts: &[Spanned<Stmt>],
        out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        for stmt in stmts {
            let is_expr = matches!(&stmt.node, Stmt::Expr(_));
            match self.exec_stmt(stmt, out)? {
                Flow::Next(val) => {
                    if is_expr && !matches!(val, Value::Nil) {
                        let display = match &val {
                            Value::Module(mid) => {
                                let m = self.native_registry.get_module(*mid);
                                format!("<module {}>", m.name)
                            }
                            Value::NativeFn(fid) => {
                                let name = self.native_registry.fn_name(*fid);
                                format!("<native-fn {name}>")
                            }
                            other => other.display(&self.types),
                        };
                        let _ = writeln!(out, "{display}");
                    }
                }
                Flow::Return { span, .. } => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::RetOutsideFunction,
                    ))
                    .at(span));
                }
                Flow::Propagate { span, .. } => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::PropagateOutsideFunction,
                    ))
                    .at(span));
                }
                Flow::Bail(span) => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::BailOutsideLoop,
                    ))
                    .at(span));
                }
                Flow::Cont(span) => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::ContOutsideLoop,
                    ))
                    .at(span));
                }
            }
        }
        Ok(())
    }

    /// Dispatch loop-body flow control. Always pops scope.
    /// Returns `None` to continue looping, `Some(flow)` to exit.
    pub(super) fn dispatch_loop_flow(
        &mut self,
        flow: Flow,
        out: &mut impl Write,
    ) -> Option<Flow> {
        match flow {
            Flow::Next(_) | Flow::Cont(_) => {
                self.pop_scope(out);
                None
            }
            Flow::Bail(_) => {
                self.pop_scope(out);
                Some(Flow::Next(Value::Nil))
            }
            ret @ (Flow::Return { .. } | Flow::Propagate { .. }) => {
                self.pop_scope(out);
                Some(ret)
            }
        }
    }
}
