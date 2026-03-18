use std::collections::HashMap;
use std::io::Write;

use indexmap::IndexMap;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use tracing::{debug, trace};

use super::ast::{
    AssignTarget, AstFieldDef, AstVariantDef, BinOp, Expr, FuncDef, MethodSig, Param, Program,
    Spanned, Stmt, UnaryOp,
};
use super::error::RuntimeError;
use super::types::{prim, TypeDef, TypeExpr, TypeId, TypeRegistry, VariantDef};
use super::value::{FuncParam, Value};

// ── Protocol constants ──────────────────────────────────────────────────────

const SELF_PARAM: &str = "self";
const METHOD_TO_ITER: &str = "to_iter";
const METHOD_NEXT: &str = "next";
const VARIANT_NONE: &str = "None";

// ── Flow ─────────────────────────────────────────────────────────────────────

/// Outcome of executing a statement or block.
#[derive(Debug)]
pub enum Flow {
    /// Statement completed normally. Carries the value for expression-statements.
    Next(Value),
    /// A `ret` statement was hit; carry the value up to the call site.
    Return(Value),
    /// A `break` was hit; exit the current loop.
    Break,
    /// A `continue` was hit; skip to the next loop iteration.
    Continue,
}

// ── Interface storage ────────────────────────────────────────────

/// A registered interface — stores method signatures for conformance checking.
#[derive(Debug, Clone)]
struct InterfaceDef {
    #[allow(dead_code)]
    type_params: Vec<String>,
    methods: Vec<MethodSig>,
}

// ── Interpreter ──────────────────────────────────────────────────────────────

/// The KataScript interpreter. Owns the type registry, variable scopes,
/// and all evaluation logic.
pub struct Interpreter {
    /// All registered types.
    pub types: TypeRegistry,
    /// Lexically-scoped variable frames. Lookup walks innermost to outermost.
    frames: Vec<IndexMap<String, Value>>,
    /// Method tables: TypeId → method_name → Func value.
    methods: HashMap<TypeId, IndexMap<String, Value>>,
    /// Interface definitions: name → method signatures.
    interfaces: IndexMap<String, InterfaceDef>,
    /// Temporary: holds mutated `self` after a method call for copy-out.
    last_method_self: Option<Value>,
}

impl Interpreter {
    /// Create a new interpreter with primitive types bootstrapped.
    pub fn new() -> Self {
        let types = TypeRegistry::new();
        let mut interp = Self {
            types,
            frames: vec![IndexMap::new()],
            methods: HashMap::new(),
            interfaces: IndexMap::new(),
            last_method_self: None,
        };

        // Populate scope with prim type values so `Int`, `Str`, etc. resolve.
        interp.set("Nil".into(), Value::Type(prim::NIL));
        interp.set("Bool".into(), Value::Type(prim::BOOL));
        interp.set("Int".into(), Value::Type(prim::INT));
        interp.set("Float".into(), Value::Type(prim::FLOAT));
        interp.set("Str".into(), Value::Type(prim::STR));
        interp.set("Bin".into(), Value::Type(prim::BIN));
        interp.set("Func".into(), Value::Type(prim::FUNC));
        interp.set("Type".into(), Value::Type(prim::TYPE));

        // The `std` namespace — std.ops.add, std.ops.sub, etc.
        interp.set("std".into(), Value::Namespace("std".into()));

        interp
    }

    // ── Scope ────────────────────────────────────────────────────────────

    fn get(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        None
    }

    fn set(&mut self, name: String, value: Value) {
        self.frames
            .last_mut()
            .expect("interpreter always has at least one frame")
            .insert(name, value);
    }

    fn push_scope(&mut self) {
        self.frames.push(IndexMap::new());
    }

    fn pop_scope(&mut self) {
        debug_assert!(self.frames.len() > 1, "cannot pop the global frame");
        self.frames.pop();
    }

    /// Update an existing variable in the nearest enclosing scope that contains it.
    fn update_in_scope(&mut self, name: &str, value: Value) -> Result<(), String> {
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(format!("undefined variable '{name}'"))
    }

    // ── Type resolution ──────────────────────────────────────────────────

    /// Resolve a type name string (from source code) to a TypeId.
    fn resolve_type(&self, name: &str) -> Result<TypeId, String> {
        // Check if it's a value in scope that holds a Type.
        if let Some(val) = self.get(name) {
            if let Value::Type(tid) = val {
                return Ok(*tid);
            }
        }
        // Check the type registry directly.
        self.types
            .lookup(name)
            .ok_or_else(|| format!("undefined type '{name}'"))
    }

    /// Resolve a type annotation expression to a TypeId.
    /// Handles bare names (`Int`) and parameterized types (`Opt[Int]`).
    fn resolve_type_expr(&mut self, expr: &Expr) -> Result<TypeId, String> {
        match expr {
            Expr::Name(n) => self.resolve_type(n),
            Expr::Item { object, args } => {
                let base_id = self.resolve_type_expr(&object.node)?;
                let mut type_args = Vec::with_capacity(args.len());
                for a in args {
                    type_args.push(self.resolve_type_expr(&a.node)?);
                }
                match self.types.get(base_id) {
                    TypeDef::Enum { .. } => self.types.instantiate_enum(base_id, type_args),
                    TypeDef::Struct { .. } => self.types.instantiate_struct(base_id, type_args),
                    _ => Err(format!(
                        "'{}' is not a generic type",
                        self.types.display_name(base_id)
                    )),
                }
            }
            _ => Err("unsupported type annotation expression".to_string()),
        }
    }

    /// Check that a value conforms to an expected type.
    fn check_type(&self, value: &Value, expected: TypeId) -> Result<(), String> {
        let actual = value.type_id();
        if actual != expected {
            return Err(format!(
                "type mismatch: expected {}, got {}",
                self.types.display_name(expected),
                self.types.display_name(actual),
            ));
        }
        Ok(())
    }

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
            self.exec_top_level(pre, "in prelude", out)?;
        }

        debug!(stmts = program.len(), "exec_program");
        self.exec_top_level(program, "outside of function", out)?;
        Ok(())
    }

    // ── Statement execution ──────────────────────────────────────────────

    fn exec_stmt(
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
                self.register_enum(name, type_params, variants)
                    .map_err(|e| RuntimeError::from(e).at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::KindDef {
                name,
                type_params,
                fields,
            } => {
                self.register_struct(name, type_params, fields)
                    .map_err(|e| RuntimeError::from(e).at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::InterfaceDef {
                name,
                type_params,
                methods,
            } => {
                self.register_interface(name, type_params, methods)
                    .map_err(|e| RuntimeError::from(e).at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Impl {
                type_name,
                as_type,
                methods,
            } => {
                self.register_impl(type_name, methods, out)
                    .map_err(|e| RuntimeError::from(e).at(stmt.span))?;
                if let Some(iface_expr) = as_type {
                    self.check_conformance(type_name, &iface_expr.node)
                        .map_err(|e| RuntimeError::from(e).at(iface_expr.span))?;
                }
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::FuncDef(FuncDef {
                name,
                params,
                ret_type,
                body,
            }) => {
                let func_params = self
                    .resolve_params(params)
                    .map_err(|e| RuntimeError::from(e).at(stmt.span))?;

                let ret_tid = ret_type
                    .as_ref()
                    .map(|ann| {
                        self.resolve_type_expr(&ann.node)
                            .map_err(|e| RuntimeError::from(e).at(ann.span))
                    })
                    .transpose()?;

                let func = Value::Func {
                    params: func_params,
                    ret_type: ret_tid,
                    body: body.clone(),
                };
                self.set(name.clone(), func);
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Let { name, value } => {
                let val = self.eval_value(value, out)?;
                self.set(name.clone(), val);
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Assign { target, value } => {
                let val = self.eval_value(value, out)?;
                match target {
                    AssignTarget::Name(name) => {
                        self.update_in_scope(name, val)
                            .map_err(|e| RuntimeError::from(e).at(stmt.span))?;
                        Ok(Flow::Next(Value::Nil))
                    }
                    AssignTarget::Attr { object, attr } => self
                        .exec_attr_assign(object, attr, val)
                        .map_err(|e| RuntimeError::from(e).at(stmt.span)),
                }
            }

            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),

            Stmt::Ret(expr) => {
                let val = self.eval_value(expr, out)?;
                Ok(Flow::Return(val))
            }
        }
    }

    // ── Block execution ──────────────────────────────────────────────────

    fn exec_block(
        &mut self,
        stmts: &[Spanned<Stmt>],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        let mut last_val = Value::Nil;
        for stmt in stmts {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(v) => last_val = v,
                flow @ (Flow::Return(_) | Flow::Break | Flow::Continue) => return Ok(flow),
            }
        }
        Ok(Flow::Next(last_val))
    }

    // ── Expression evaluation ────────────────────────────────────────────

    fn eval_expr(
        &mut self,
        expr: &Spanned<Expr>,
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        trace!(?expr.node, "eval_expr");
        match &expr.node {
            Expr::Nil => Ok(Flow::Next(Value::Nil)),
            Expr::Bool(b) => Ok(Flow::Next(Value::Bool(*b))),
            Expr::Int(s) => {
                let n: BigInt = s
                    .parse()
                    .map_err(|e| format!("invalid integer literal '{s}': {e}"))
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(Value::Int(n)))
            }
            Expr::Float(s) => {
                let n: f64 = s
                    .parse()
                    .map_err(|e| format!("invalid float literal '{s}': {e}"))
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(Value::Float(n)))
            }
            Expr::Str(s) => Ok(Flow::Next(Value::Str(s.clone()))),

            Expr::Name(name) => self.get(name).cloned().map(Flow::Next).ok_or_else(|| {
                RuntimeError::new(format!("undefined variable '{name}'")).at(expr.span)
            }),

            Expr::With { bindings, body } => {
                self.push_scope();
                for (name, val_expr) in bindings {
                    let val = self.eval_value(val_expr, out)?;
                    self.set(name.clone(), val);
                }
                let result = self.exec_block(body, out);
                self.pop_scope();
                result
            }

            Expr::Attr { object, name } => {
                let obj = self.eval_value(object, out)?;
                self.eval_attr(&obj, name)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))
            }

            Expr::Item { object, args } => {
                let obj = self.eval_value(object, out)?;
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
                }
                self.eval_item(&obj, &arg_vals)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))
            }

            Expr::Call { callee, args } => {
                // Evaluate args once.
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
                }

                // Fast path: if callee is a bare Name, check builtins first.
                if let Expr::Name(name) = &callee.node {
                    if let Some(result) = self
                        .call_builtin(name, &arg_vals, out)
                        .map_err(|e| RuntimeError::from(e).at(expr.span))?
                    {
                        return Ok(result);
                    }
                }

                let func = self.eval_value(callee, out)?;

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

                let result = self
                    .eval_call(func, &arg_vals, out)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;

                // Copy-in copy-out: write mutated self back to the receiver variable.
                if let Some(var_name) = receiver_var {
                    if let Some(mutated_self) = self.last_method_self.take() {
                        self.update_in_scope(&var_name, mutated_self)
                            .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                    }
                }

                Ok(result)
            }

            Expr::BinOp { op, left, right } => {
                let lv = self.eval_value(left, out)?;
                let rv = self.eval_value(right, out)?;
                let result = Self::eval_binop(*op, &lv, &rv)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(result))
            }

            Expr::If {
                cond,
                then_body,
                else_body,
            } => {
                let cv = self.eval_value(cond, out)?;
                if Self::truth(&cv) {
                    self.push_scope();
                    let result = self.exec_block(then_body, out);
                    self.pop_scope();
                    result
                } else if let Some(else_stmts) = else_body {
                    self.push_scope();
                    let result = self.exec_block(else_stmts, out);
                    self.pop_scope();
                    result
                } else {
                    Ok(Flow::Next(Value::Nil))
                }
            }

            Expr::For {
                binding,
                iter_expr,
                body,
            } => {
                // 1. Evaluate the iterable expression.
                let iterable = self.eval_value(iter_expr, out)?;

                // 2. Call .to_iter() on it.
                let iter_val = self
                    .call_method(&iterable, METHOD_TO_ITER, &[], out)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;

                // 3. Loop: call .next() on the iterator.
                // The iterator lives as a Rust local — no synthetic variable in scope.
                let mut iterator = iter_val;
                loop {
                    let bound = self
                        .resolve_method(&iterator, METHOD_NEXT)
                        .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                    let next_result = match self
                        .eval_call(bound, &[], out)
                        .map_err(|e| RuntimeError::from(e).at(expr.span))?
                    {
                        Flow::Next(v) => v,
                        _ => break,
                    };

                    // Copy-out: update the local iterator with mutated self.
                    if let Some(mutated_self) = self.last_method_self.take() {
                        iterator = mutated_self;
                    }

                    // 4. Check if result is Opt.None.
                    let Value::Enum {
                        type_id: opt_tid,
                        variant_idx,
                        fields,
                    } = &next_result
                    else {
                        return Err(
                            RuntimeError::new("iterator .next() must return an Opt value")
                                .at(expr.span),
                        );
                    };

                    if self.types.variant_name(*opt_tid, *variant_idx) == VARIANT_NONE {
                        break;
                    }

                    // 5. Extract the value from Some(val).
                    let val = fields
                        .first()
                        .cloned()
                        .ok_or_else(|| RuntimeError::new("Opt.Some has no field").at(expr.span))?;

                    // Bind loop variable and execute body.
                    self.push_scope();
                    self.set(binding.clone(), val);

                    let flow = self.exec_block(body, out)?;
                    if let Some(early) = self.dispatch_loop_flow(flow) {
                        return Ok(early);
                    }
                }

                Ok(Flow::Next(Value::Nil))
            }

            Expr::While { cond, body } => {
                loop {
                    let cv = self.eval_value(cond, out)?;
                    if !Self::truth(&cv) {
                        break;
                    }
                    self.push_scope();
                    let flow = self.exec_block(body, out)?;
                    if let Some(early) = self.dispatch_loop_flow(flow) {
                        return Ok(early);
                    }
                }
                Ok(Flow::Next(Value::Nil))
            }

            Expr::And { left, right } => {
                let lv = self.eval_value(left, out)?;
                if !Self::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = self.eval_value(right, out)?;
                Ok(Flow::Next(rv))
            }

            Expr::Or { left, right } => {
                let lv = self.eval_value(left, out)?;
                if Self::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = self.eval_value(right, out)?;
                Ok(Flow::Next(rv))
            }

            Expr::UnaryOp { op, operand } => {
                let val = self.eval_value(operand, out)?;
                let result = Self::eval_unaryop(*op, &val)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(result))
            }

            Expr::Construct { type_expr, fields } => {
                let type_val = self.eval_value(type_expr, out)?;
                let Value::Type(type_id) = type_val else {
                    return Err(RuntimeError::new(format!(
                        "cannot construct '{}' — not a type",
                        self.types.display_name(type_val.type_id())
                    ))
                    .at(expr.span));
                };

                let expected_fields = self
                    .types
                    .get_struct_fields(type_id)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?
                    .clone();

                // Check for extra fields.
                for (fname, _) in fields {
                    if !expected_fields.contains_key(fname.as_str()) {
                        return Err(RuntimeError::new(format!(
                            "'{}' has no field '{fname}'",
                            self.types.display_name(type_id)
                        ))
                        .at(expr.span));
                    }
                }

                // Check for missing fields and build in definition order.
                let mut provided: IndexMap<String, Value> = IndexMap::new();
                for (fname, fexpr) in fields {
                    let val = self.eval_value(fexpr, out)?;
                    provided.insert(fname.clone(), val);
                }

                let mut result_fields = IndexMap::new();
                for (fname, expected_tid) in &expected_fields {
                    let val = provided.shift_remove(fname.as_str()).ok_or_else(|| {
                        RuntimeError::new(format!(
                            "missing field '{fname}' in '{}' construction",
                            self.types.display_name(type_id)
                        ))
                        .at(expr.span)
                    })?;
                    self.check_type(&val, *expected_tid)
                        .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                    result_fields.insert(fname.clone(), val);
                }

                Ok(Flow::Next(Value::Struct {
                    type_id,
                    fields: result_fields,
                }))
            }
        }
    }

    /// Evaluate an expression and extract the value (not a return flow).
    fn eval_value(
        &mut self,
        expr: &Spanned<Expr>,
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        match self.eval_expr(expr, out)? {
            Flow::Next(v) | Flow::Return(v) => Ok(v),
            Flow::Break => Err(RuntimeError::new("break outside of loop").at(expr.span)),
            Flow::Continue => Err(RuntimeError::new("continue outside of loop").at(expr.span)),
        }
    }

    // ── Enum construction ────────────────────────────────────────────────

    fn register_enum(
        &mut self,
        name: &str,
        type_params: &[String],
        ast_variants: &[AstVariantDef],
    ) -> Result<(), String> {
        let mut variants = IndexMap::new();
        for v in ast_variants {
            let fields = v
                .fields
                .iter()
                .map(|f| self.resolve_type_ann(&f.node, type_params))
                .collect::<Result<Vec<_>, _>>()?;
            variants.insert(v.name.clone(), VariantDef { fields });
        }

        let type_id = self
            .types
            .register_enum(name.to_string(), type_params.to_vec(), variants);

        // Put the type in scope as a Value::Type.
        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
    }

    // ── Struct registration ────────────────────────────────────────────

    fn register_struct(
        &mut self,
        name: &str,
        type_params: &[String],
        ast_fields: &[AstFieldDef],
    ) -> Result<(), String> {
        let mut fields = IndexMap::new();
        for f in ast_fields {
            let texpr = self.resolve_type_ann(&f.type_ann.node, type_params)?;
            fields.insert(f.name.clone(), texpr);
        }

        let type_id = self
            .types
            .register_struct(name.to_string(), type_params.to_vec(), fields);

        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
    }

    // ── Interface registration ─────────────────────────────────────

    fn register_interface(
        &mut self,
        name: &str,
        type_params: &[String],
        methods: &[MethodSig],
    ) -> Result<(), String> {
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
    fn check_conformance(&self, type_name: &str, iface_expr: &Expr) -> Result<(), String> {
        // Resolve the interface name (ignoring type args for now).
        let iface_name = match iface_expr {
            Expr::Name(n) => n.as_str(),
            Expr::Item { object, .. } => {
                if let Expr::Name(n) = &object.node {
                    n.as_str()
                } else {
                    return Err("invalid interface expression".to_string());
                }
            }
            _ => return Err("invalid interface expression".to_string()),
        };

        let iface = self
            .interfaces
            .get(iface_name)
            .ok_or_else(|| format!("undefined interface '{iface_name}'"))?
            .clone();

        let type_id = self.resolve_type(type_name)?;
        let method_table = self
            .methods
            .get(&type_id)
            .ok_or_else(|| format!("'{type_name}' has no methods"))?;

        for sig in &iface.methods {
            let func = method_table.get(&sig.name).ok_or_else(|| {
                format!(
                    "'{type_name}' does not implement '{iface_name}': missing method '{}'",
                    sig.name
                )
            })?;

            // Verify param count matches (including self).
            if let Value::Func { params, .. } = func {
                if params.len() != sig.params.len() {
                    return Err(format!(
                        "'{type_name}' does not implement '{iface_name}': method '{}' expects {} param(s), got {}",
                        sig.name,
                        sig.params.len(),
                        params.len()
                    ));
                }
            }
        }

        Ok(())
    }

    // ── Impl registration ────────────────────────────────────────────

    fn register_impl(
        &mut self,
        type_name: &str,
        methods: &[Spanned<FuncDef>],
        _out: &mut impl Write,
    ) -> Result<(), String> {
        let type_id = self.resolve_type(type_name)?;

        for method in methods {
            let FuncDef {
                name,
                params,
                ret_type,
                body,
            } = &method.node;

            if params.is_empty() || params[0].name != SELF_PARAM {
                return Err(format!(
                    "method '{name}' must have 'self' as first parameter"
                ));
            }

            let func_params = self.resolve_params(params)?;

            let ret_tid = ret_type
                .as_ref()
                .map(|ann| self.resolve_type_expr(&ann.node))
                .transpose()?;

            let func = Value::Func {
                params: func_params,
                ret_type: ret_tid,
                body: body.clone(),
            };

            self.methods
                .entry(type_id)
                .or_insert_with(IndexMap::new)
                .insert(name.clone(), func);
        }

        Ok(())
    }

    /// Convert an expression used as a type annotation to a TypeExpr.
    fn resolve_type_ann(&self, expr: &Expr, type_params: &[String]) -> Result<TypeExpr, String> {
        match expr {
            Expr::Name(n) => {
                if type_params.contains(n) {
                    Ok(TypeExpr::Param(n.clone()))
                } else {
                    Ok(TypeExpr::Concrete(self.resolve_type(n)?))
                }
            }
            _ => Err("complex type annotations not yet supported".to_string()),
        }
    }

    // ── Attr assignment: a.b = v ─────────────────────────────────────

    fn exec_attr_assign(
        &mut self,
        object: &Spanned<Expr>,
        attr: &str,
        val: Value,
    ) -> Result<Flow, String> {
        // Only handle Expr::Name(var).attr = val for now.
        let Expr::Name(var_name) = &object.node else {
            return Err("nested attr assignment not yet supported".to_string());
        };

        // Phase 1: read the struct, validate types.
        let (type_id, expected_tid) = {
            let current = self
                .get(var_name)
                .ok_or_else(|| format!("undefined variable '{var_name}'"))?;
            let Value::Struct { type_id, .. } = current else {
                return Err(format!(
                    "cannot assign to field '.{attr}' on {}",
                    self.types.display_name(current.type_id())
                ));
            };
            let struct_fields = self.types.get_struct_fields(*type_id)?;
            let expected_tid = struct_fields.get(attr).copied().ok_or_else(|| {
                format!(
                    "'{}' has no field '{attr}'",
                    self.types.display_name(*type_id)
                )
            })?;
            (*type_id, expected_tid)
        };

        self.check_type(&val, expected_tid)?;

        // Phase 2: mutate the struct field in scope.
        for frame in self.frames.iter_mut().rev() {
            if let Some(entry) = frame.get_mut(var_name) {
                if let Value::Struct {
                    type_id: tid,
                    fields,
                } = entry
                {
                    debug_assert_eq!(*tid, type_id);
                    fields.insert(attr.to_string(), val);
                    return Ok(Flow::Next(Value::Nil));
                }
            }
        }
        Err(format!("undefined variable '{var_name}'"))
    }

    // ── Method helpers ────────────────────────────────────────────────

    /// Look up a method by name in the method table for a given type.
    fn lookup_method(&self, type_id: TypeId, name: &str) -> Option<Value> {
        self.methods
            .get(&type_id)
            .and_then(|table| table.get(name))
            .cloned()
    }

    /// Wrap a Func value as a BoundMethod with the given receiver.
    fn bind_method(&self, receiver: Value, method: Value, name: &str) -> Result<Value, String> {
        if !matches!(method, Value::Func { .. }) {
            return Err(format!(
                "method table entry '{name}' on '{}' is not a function",
                self.types.display_name(receiver.type_id())
            ));
        }
        Ok(Value::BoundMethod {
            receiver: Box::new(receiver),
            func: Box::new(method),
        })
    }

    /// Look up and bind a method, ready to call.
    fn resolve_method(&self, receiver: &Value, name: &str) -> Result<Value, String> {
        let tid = receiver.type_id();
        let func = self
            .lookup_method(tid, name)
            .ok_or_else(|| format!("'{}' has no method '{name}'", self.types.display_name(tid)))?;
        self.bind_method(receiver.clone(), func, name)
    }

    /// Call a method on a value by name (no copy-out — caller handles that).
    fn call_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Value, String> {
        let bound = self.resolve_method(receiver, name)?;
        match self.eval_call(bound, args, out)? {
            Flow::Next(v) | Flow::Return(v) => Ok(v),
            _ => Err(format!("method '{name}' returned abnormal flow")),
        }
    }

    // ── Attr: a.b ─────────────────────────────────────────────────────

    fn eval_attr(&self, object: &Value, name: &str) -> Result<Flow, String> {
        match object {
            // Type.Variant — enum variant access
            Value::Type(type_id) => {
                let def = self.types.get(*type_id);
                match def {
                    TypeDef::EnumInstance { variants, .. } => {
                        let (idx, _, vdef) = variants.get_full(name).ok_or_else(|| {
                            format!(
                                "'{}' has no variant '{name}'",
                                self.types.display_name(*type_id)
                            )
                        })?;
                        let variant_idx = idx as u32;

                        if vdef.fields.is_empty() {
                            // Unit variant — return the enum value directly.
                            Ok(Flow::Next(Value::Enum {
                                type_id: *type_id,
                                variant_idx,
                                fields: vec![],
                            }))
                        } else {
                            // Data variant — return a constructor.
                            Ok(Flow::Next(Value::VariantConstructor {
                                type_id: *type_id,
                                variant_idx,
                                field_types: vdef.fields.clone(),
                            }))
                        }
                    }
                    TypeDef::StructInstance { .. } => Err(format!(
                        "'{}' is a struct type — construct with `{} {{ ... }}`",
                        self.types.display_name(*type_id),
                        self.types.display_name(*type_id),
                    )),
                    _ => Err(format!(
                        "cannot access '.{name}' on type '{}'",
                        self.types.display_name(*type_id)
                    )),
                }
            }
            // Struct field access — p.x, with method fallback
            Value::Struct { type_id, fields } => {
                // Field access first.
                if let Some(val) = fields.get(name) {
                    return Ok(Flow::Next(val.clone()));
                }
                // Method lookup.
                if let Ok(bound) = self.resolve_method(object, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(format!(
                    "'{}' has no field or method '{name}'",
                    self.types.display_name(*type_id)
                ))
            }
            // Namespace.child — e.g., std.ops, std.ops.add
            Value::Namespace(ns) => {
                let qualified = format!("{ns}.{name}");
                // Known sub-namespaces return another Namespace;
                // everything else is a builtin function.
                match qualified.as_str() {
                    "std.ops" => Ok(Flow::Next(Value::Namespace(qualified))),
                    _ => Ok(Flow::Next(Value::BuiltinFn(qualified))),
                }
            }

            other => {
                // Check method table for any value type.
                if let Ok(bound) = self.resolve_method(other, name) {
                    return Ok(Flow::Next(bound));
                }
                Err(format!(
                    "cannot access '.{name}' on {}",
                    self.types.display_name(other.type_id())
                ))
            }
        }
    }

    // ── Item: a[b] ───────────────────────────────────────────────────────

    fn eval_item(&mut self, object: &Value, args: &[Value]) -> Result<Flow, String> {
        match object {
            // Type[Args] — generic instantiation (enum or struct)
            Value::Type(base_id) => {
                let type_args: Vec<TypeId> = args
                    .iter()
                    .map(|v| match v {
                        Value::Type(tid) => Ok(*tid),
                        other => Err(format!(
                            "expected a type argument, got {}",
                            self.types.display_name(other.type_id())
                        )),
                    })
                    .collect::<Result<_, _>>()?;
                let instance_id = match self.types.get(*base_id) {
                    TypeDef::Enum { .. } => self.types.instantiate_enum(*base_id, type_args)?,
                    TypeDef::Struct { .. } => self.types.instantiate_struct(*base_id, type_args)?,
                    _ => {
                        return Err(format!(
                            "'{}' is not a generic type",
                            self.types.display_name(*base_id)
                        ))
                    }
                };
                Ok(Flow::Next(Value::Type(instance_id)))
            }
            other => Err(format!(
                "cannot index into {}",
                self.types.display_name(other.type_id())
            )),
        }
    }

    // ── Call: a(b) ───────────────────────────────────────────────────────

    fn eval_call(
        &mut self,
        func: Value,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Flow, String> {
        match func {
            Value::Func {
                params,
                ret_type,
                body,
            } => {
                if args.len() != params.len() {
                    return Err(format!(
                        "function expects {} argument(s), got {}",
                        params.len(),
                        args.len()
                    ));
                }

                let result = self.call_func_body(&params, args, ret_type, &body, false, out)?;
                Ok(Flow::Next(result))
            }

            Value::VariantConstructor {
                type_id,
                variant_idx,
                field_types,
            } => {
                if args.len() != field_types.len() {
                    let variant_name = self.types.variant_name(type_id, variant_idx);
                    return Err(format!(
                        "'{variant_name}' expects {} argument(s), got {}",
                        field_types.len(),
                        args.len()
                    ));
                }

                // Type-check each field.
                for (val, &expected) in args.iter().zip(field_types.iter()) {
                    self.check_type(val, expected)?;
                }

                Ok(Flow::Next(Value::Enum {
                    type_id,
                    variant_idx,
                    fields: args.to_vec(),
                }))
            }

            Value::BoundMethod { receiver, func } => {
                let Value::Func {
                    params,
                    ret_type,
                    body,
                } = *func
                else {
                    return Err("bound method does not wrap a Func".to_string());
                };

                // params[0] is self (pre-bound); user-visible args match params[1..].
                let method_params = &params[1..];
                if args.len() != method_params.len() {
                    return Err(format!(
                        "method expects {} argument(s), got {}",
                        method_params.len(),
                        args.len()
                    ));
                }

                // Build full arg list: [self] + user args.
                let mut full_args = Vec::with_capacity(params.len());
                full_args.push(*receiver);
                full_args.extend_from_slice(args);

                let result =
                    self.call_func_body(&params, &full_args, ret_type, &body, true, out)?;
                Ok(Flow::Next(result))
            }

            Value::BuiltinFn(name) => self.call_builtin_fn(&name, args),

            other => Err(format!(
                "'{}' is not callable",
                self.types.display_name(other.type_id())
            )),
        }
    }

    // ── Operators ─────────────────────────────────────────────────────────

    /// Truthiness: nil, false, 0, 0.0, "" are falsy; everything else is truthy.
    fn truth(val: &Value) -> bool {
        match val {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != BigInt::ZERO,
            Value::Float(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    fn eval_unaryop(op: UnaryOp, val: &Value) -> Result<Value, String> {
        match op {
            UnaryOp::Neg => match val {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(n) => Ok(Value::Float(-n)),
                other => Err(format!(
                    "cannot negate {}",
                    other.type_id().display_static()
                )),
            },
            UnaryOp::Not => Ok(Value::Bool(!Self::truth(val))),
        }
    }

    fn eval_binop(op: BinOp, left: &Value, right: &Value) -> Result<Value, String> {
        match op {
            BinOp::Add => Self::op_add(left, right),
            BinOp::Sub => Self::op_arith(left, right, "sub", |a, b| a - b, |a, b| a - b),
            BinOp::Mul => Self::op_arith(left, right, "mul", |a, b| a * b, |a, b| a * b),
            BinOp::Div => Self::op_div(left, right),
            BinOp::Eq => Ok(Value::Bool(left == right || Self::cross_eq(left, right))),
            BinOp::Ne => Ok(Value::Bool(left != right && !Self::cross_eq(left, right))),
            BinOp::Lt => Self::op_cmp(left, right, "lt", |o| o.is_lt()),
            BinOp::Gt => Self::op_cmp(left, right, "gt", |o| o.is_gt()),
            BinOp::Le => Self::op_cmp(left, right, "le", |o| !o.is_gt()),
            BinOp::Ge => Self::op_cmp(left, right, "ge", |o| !o.is_lt()),
        }
    }

    fn op_add(left: &Value, right: &Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => {
                Ok(Value::Float(a.to_f64().unwrap_or(f64::NAN) + b))
            }
            (Value::Float(a), Value::Int(b)) => {
                Ok(Value::Float(a + b.to_f64().unwrap_or(f64::NAN)))
            }
            (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),
            _ => Err(format!(
                "cannot add {} and {}",
                left.type_id().display_static(),
                right.type_id().display_static(),
            )),
        }
    }

    fn op_arith(
        left: &Value,
        right: &Value,
        name: &str,
        int_op: impl Fn(&BigInt, &BigInt) -> BigInt,
        float_op: impl Fn(f64, f64) -> f64,
    ) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(a, b))),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(*a, *b))),
            (Value::Int(a), Value::Float(b)) => {
                Ok(Value::Float(float_op(a.to_f64().unwrap_or(f64::NAN), *b)))
            }
            (Value::Float(a), Value::Int(b)) => {
                Ok(Value::Float(float_op(*a, b.to_f64().unwrap_or(f64::NAN))))
            }
            _ => Err(format!(
                "cannot {name} {} and {}",
                left.type_id().display_static(),
                right.type_id().display_static(),
            )),
        }
    }

    fn op_div(left: &Value, right: &Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(_), Value::Int(b)) if *b == BigInt::ZERO => {
                Err("division by zero".to_string())
            }
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => {
                Ok(Value::Float(a.to_f64().unwrap_or(f64::NAN) / b))
            }
            (Value::Float(a), Value::Int(b)) => {
                Ok(Value::Float(a / b.to_f64().unwrap_or(f64::NAN)))
            }
            _ => Err(format!(
                "cannot div {} and {}",
                left.type_id().display_static(),
                right.type_id().display_static(),
            )),
        }
    }

    /// Cross-type equality for Int/Float promotion.
    fn cross_eq(left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(a), Value::Float(b)) => a.to_f64().map_or(false, |a| a == *b),
            (Value::Float(a), Value::Int(b)) => b.to_f64().map_or(false, |b| *a == b),
            _ => false,
        }
    }

    fn op_cmp(
        left: &Value,
        right: &Value,
        name: &str,
        pred: impl Fn(std::cmp::Ordering) -> bool,
    ) -> Result<Value, String> {
        let ord = match (left, right) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a
                .partial_cmp(b)
                .ok_or_else(|| "cannot compare NaN".to_string())?,
            (Value::Int(a), Value::Float(b)) => {
                let af = a.to_f64().unwrap_or(f64::NAN);
                af.partial_cmp(b)
                    .ok_or_else(|| "cannot compare NaN".to_string())?
            }
            (Value::Float(a), Value::Int(b)) => {
                let bf = b.to_f64().unwrap_or(f64::NAN);
                a.partial_cmp(&bf)
                    .ok_or_else(|| "cannot compare NaN".to_string())?
            }
            (Value::Str(a), Value::Str(b)) => a.cmp(b),
            _ => {
                return Err(format!(
                    "cannot {name} {} and {}",
                    left.type_id().display_static(),
                    right.type_id().display_static(),
                ))
            }
        };
        Ok(Value::Bool(pred(ord)))
    }

    /// Dispatch a named builtin function (from `std.ops.*` namespace).
    fn call_builtin_fn(&self, name: &str, args: &[Value]) -> Result<Flow, String> {
        let suffix = name
            .strip_prefix("std.ops.")
            .ok_or_else(|| format!("unknown builtin function '{name}'"))?;

        // Binary ops: match suffix against BinOp::method_name().
        const BINOPS: [BinOp; 10] = [
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::Eq,
            BinOp::Ne,
            BinOp::Lt,
            BinOp::Gt,
            BinOp::Le,
            BinOp::Ge,
        ];
        for op in BINOPS {
            if suffix == op.method_name() {
                if args.len() != 2 {
                    return Err(format!("{name} expects 2 arguments, got {}", args.len()));
                }
                return Ok(Flow::Next(Self::eval_binop(op, &args[0], &args[1])?));
            }
        }

        // Unary ops and special functions.
        match suffix {
            "neg" | "not" => {
                if args.len() != 1 {
                    return Err(format!("{name} expects 1 argument, got {}", args.len()));
                }
                let op = if suffix == "neg" {
                    UnaryOp::Neg
                } else {
                    UnaryOp::Not
                };
                Ok(Flow::Next(Self::eval_unaryop(op, &args[0])?))
            }
            "truth" => {
                if args.len() != 1 {
                    return Err(format!("{name} expects 1 argument, got {}", args.len()));
                }
                Ok(Flow::Next(Value::Bool(Self::truth(&args[0]))))
            }
            _ => Err(format!("unknown builtin function '{name}'")),
        }
    }

    // ── Built-in functions ───────────────────────────────────────────────

    fn call_builtin(
        &mut self,
        name: &str,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Option<Flow>, String> {
        match name {
            "print" => {
                let parts: Vec<String> = args.iter().map(|v| v.display(&self.types)).collect();
                writeln!(out, "{}", parts.join(" ")).map_err(|e| e.to_string())?;
                Ok(Some(Flow::Next(Value::Nil)))
            }
            "typeof" => {
                if args.len() != 1 {
                    return Err(format!("typeof expects 1 argument, got {}", args.len()));
                }
                let tid = args[0].type_id();
                Ok(Some(Flow::Next(Value::Type(tid))))
            }
            _ => Ok(None),
        }
    }

    // ── Shared helpers ──────────────────────────────────────────────────

    /// Resolve AST params to FuncParam values.
    fn resolve_params(&mut self, params: &[Param]) -> Result<Vec<FuncParam>, String> {
        params
            .iter()
            .map(|p| {
                let type_id = p
                    .type_ann
                    .as_ref()
                    .map(|ann| self.resolve_type_expr(&ann.node))
                    .transpose()?;
                Ok(FuncParam {
                    name: p.name.clone(),
                    type_id,
                })
            })
            .collect()
    }

    /// Execute a top-level statement list, rejecting ret/break/continue.
    fn exec_top_level(
        &mut self,
        stmts: &[Spanned<Stmt>],
        context: &str,
        out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        for stmt in stmts {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(_) => {}
                Flow::Return(_) => {
                    return Err(RuntimeError::new(format!("ret {context}")).at(stmt.span))
                }
                Flow::Break => return Err(RuntimeError::new("break outside of loop").at(stmt.span)),
                Flow::Continue => {
                    return Err(RuntimeError::new("continue outside of loop").at(stmt.span))
                }
            }
        }
        Ok(())
    }

    /// Execute a function body: type-check args, push scope, bind params,
    /// run body, pop scope, type-check return value.
    ///
    /// When `is_method` is true, stashes the final value of `self` in
    /// `last_method_self` before popping the scope (copy-out semantics).
    fn call_func_body(
        &mut self,
        params: &[FuncParam],
        args: &[Value],
        ret_type: Option<TypeId>,
        body: &[Spanned<Stmt>],
        is_method: bool,
        out: &mut impl Write,
    ) -> Result<Value, String> {
        for (param, val) in params.iter().zip(args.iter()) {
            if let Some(expected) = param.type_id {
                self.check_type(val, expected)?;
            }
        }

        self.push_scope();
        for (param, val) in params.iter().zip(args.iter()) {
            self.set(param.name.clone(), val.clone());
        }

        let result = match self.exec_block(body, out) {
            Ok(Flow::Next(v) | Flow::Return(v)) => v,
            Ok(Flow::Break) => {
                self.pop_scope();
                return Err("break outside of loop".to_string());
            }
            Ok(Flow::Continue) => {
                self.pop_scope();
                return Err("continue outside of loop".to_string());
            }
            Err(e) => {
                self.pop_scope();
                return Err(e.message);
            }
        };

        if is_method {
            self.last_method_self = self.get(SELF_PARAM).cloned();
        }

        self.pop_scope();

        if let Some(expected_ret) = ret_type {
            self.check_type(&result, expected_ret)?;
        }

        Ok(result)
    }

    /// Dispatch loop-body flow control. Always pops scope.
    /// Returns `None` to continue looping, `Some(flow)` to exit.
    fn dispatch_loop_flow(&mut self, flow: Flow) -> Option<Flow> {
        match flow {
            Flow::Next(_) | Flow::Continue => {
                self.pop_scope();
                None
            }
            Flow::Break => {
                self.pop_scope();
                Some(Flow::Next(Value::Nil))
            }
            ret @ Flow::Return(_) => {
                self.pop_scope();
                Some(ret)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ks::ast::{Expr, Stmt};

    fn s<T>(node: T) -> Spanned<T> {
        Spanned::new(node, (0, 0))
    }

    fn program(stmts: Vec<Spanned<Stmt>>) -> Program {
        stmts
    }

    fn call_stmt(callee: &str, args: Vec<Spanned<Expr>>) -> Spanned<Stmt> {
        s(Stmt::Expr(s(Expr::Call {
            callee: Box::new(s(Expr::Name(callee.to_string()))),
            args,
        })))
    }

    #[test]
    fn eval_int_literal() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Int("42".into()))])]);
        let mut interp = Interpreter::new();
        let mut buf = Vec::new();
        interp.exec_program(&prog, None, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "42\n");
    }

    #[test]
    fn eval_float_literal() {
        let prog = program(vec![call_stmt(
            "print",
            vec![s(Expr::Float("3.14".into()))],
        )]);
        let mut interp = Interpreter::new();
        let mut buf = Vec::new();
        interp.exec_program(&prog, None, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "3.14\n");
    }

    #[test]
    fn eval_string_literal() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Str("hello".into()))])]);
        let mut interp = Interpreter::new();
        let mut buf = Vec::new();
        interp.exec_program(&prog, None, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "hello\n");
    }

    #[test]
    fn eval_nil_literal() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Nil)])]);
        let mut interp = Interpreter::new();
        assert!(interp.exec_program(&prog, None, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_bool_literals() {
        let prog = program(vec![call_stmt(
            "print",
            vec![s(Expr::Bool(true)), s(Expr::Bool(false))],
        )]);
        let mut interp = Interpreter::new();
        assert!(interp.exec_program(&prog, None, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_undefined_variable_is_error() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Name("x".into()))])]);
        let mut interp = Interpreter::new();
        assert!(interp.exec_program(&prog, None, &mut Vec::new()).is_err());
    }

    #[test]
    fn eval_unknown_function_is_error() {
        let prog = program(vec![call_stmt("undefined_fn", vec![])]);
        let mut interp = Interpreter::new();
        let err = interp
            .exec_program(&prog, None, &mut Vec::new())
            .unwrap_err();
        assert!(
            err.message.contains("undefined variable"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn eval_empty_program() {
        let mut interp = Interpreter::new();
        assert!(interp
            .exec_program(&program(vec![]), None, &mut Vec::new())
            .is_ok());
    }

    #[test]
    fn eval_ret_outside_function_is_error() {
        let prog = program(vec![s(Stmt::Ret(s(Expr::Int("42".into()))))]);
        let mut interp = Interpreter::new();
        let err = interp
            .exec_program(&prog, None, &mut Vec::new())
            .unwrap_err();
        assert!(
            err.message.contains("ret outside of function"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn typeof_builtin() {
        let prog = program(vec![call_stmt(
            "print",
            vec![s(Expr::Call {
                callee: Box::new(s(Expr::Name("typeof".into()))),
                args: vec![s(Expr::Int("42".into()))],
            })],
        )]);
        let mut interp = Interpreter::new();
        let mut buf = Vec::new();
        interp.exec_program(&prog, None, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "Int\n");
    }

    #[test]
    fn type_names_are_values() {
        // `print(Int)` should print "Int"
        let prog = program(vec![call_stmt("print", vec![s(Expr::Name("Int".into()))])]);
        let mut interp = Interpreter::new();
        let mut buf = Vec::new();
        interp.exec_program(&prog, None, &mut buf).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "Int\n");
    }
}
