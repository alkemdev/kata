use std::collections::{HashMap, HashSet};
use std::io::Write;

use indexmap::IndexMap;
use num_bigint::BigInt;
use tracing::{debug, trace};

use super::ast::{
    AssignTarget, AstFieldDef, AstVariantDef, Expr, FuncDef, InterpPart, MethodSig, Param, Program,
    Spanned, Stmt,
};
use super::error::{
    AccessKind, ArityTarget, ConformanceError, ErrorKind, FlowMisuse, MethodDefError, NameKind,
    RuntimeError, TypeKindExpectation,
};
use super::native::{self, ModuleItem, NativeCtx, NativeFnRegistry};
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
    /// TypeIds that implement the Drop protocol.
    drop_types: HashSet<TypeId>,
    /// Suppress drop dispatch during drop execution (prevents infinite recursion).
    dropping: bool,
    /// True inside `unsafe { ... }` blocks. Gates native functions that require unsafe.
    in_unsafe: bool,
    /// Runtime heap: allocation table for Ptr handles.
    allocations: Vec<Option<Vec<Value>>>,
    /// Native function registry and module tree.
    native_registry: NativeFnRegistry,
}

impl Interpreter {
    /// Create a new interpreter with primitive types bootstrapped.
    pub fn new() -> Self {
        let types = TypeRegistry::new();
        // Bootstrap native functions and module tree.
        let boot = native::bootstrap();

        let mut interp = Self {
            types,
            frames: vec![IndexMap::new()],
            methods: HashMap::new(),
            interfaces: IndexMap::new(),
            last_method_self: None,
            drop_types: HashSet::new(),
            dropping: false,
            in_unsafe: false,
            allocations: Vec::new(),
            native_registry: boot.registry,
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

        // Top-level native functions.
        interp.set("print".into(), Value::NativeFn(boot.print_id));
        interp.set("typeof".into(), Value::NativeFn(boot.typeof_id));

        // Module tree: `std.ops.*`, `std.mem.*`.
        interp.set("std".into(), Value::Module(boot.std_module));

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

    /// Remove a name from the current (innermost) scope frame.
    fn remove(&mut self, name: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.shift_remove(name);
        }
    }

    fn push_scope(&mut self) {
        self.frames.push(IndexMap::new());
    }

    fn pop_scope(&mut self, out: &mut impl Write) {
        debug_assert!(self.frames.len() > 1, "cannot pop the global frame");
        let frame = self.frames.pop().unwrap();
        if !self.dropping {
            for (_name, value) in frame {
                self.drop_value(value, out);
            }
        }
    }

    /// Call `drop` on a value if its type implements Drop, then recursively
    /// drop struct fields. Outer drop runs first, then fields (Rust order).
    /// Suppresses nested drop dispatch to prevent infinite recursion.
    fn drop_value(&mut self, value: Value, out: &mut impl Write) {
        let tid = value.type_id();
        if self.drop_types.contains(&tid) {
            self.dropping = true;
            // Best-effort: call drop, ignore errors (destructors shouldn't fail).
            let _ = self.call_method(&value, "drop", &[], out);
            self.dropping = false;
        }
        // Recursively drop struct fields.
        if let Value::Struct { fields, .. } = value {
            for (_, field_val) in fields {
                self.drop_value(field_val, out);
            }
        }
    }

    /// Update an existing variable in the nearest enclosing scope that contains it.
    /// Returns the old value (if any) for drop dispatch by the caller.
    fn update_in_scope(&mut self, name: &str, value: Value) -> Result<Option<Value>, RuntimeError> {
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                let old = frame.insert(name.to_string(), value);
                return Ok(old);
            }
        }
        Err(ErrorKind::Undefined {
            kind: NameKind::Variable,
            name: name.to_string(),
        }
        .into())
    }

    // ── Type resolution ──────────────────────────────────────────────────

    /// Resolve a type name string (from source code) to a TypeId.
    fn resolve_type(&self, name: &str) -> Result<TypeId, RuntimeError> {
        // Check if it's a value in scope that holds a Type.
        if let Some(val) = self.get(name) {
            if let Value::Type(tid) = val {
                return Ok(*tid);
            }
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
    fn resolve_type_expr(&mut self, expr: &Expr) -> Result<TypeId, RuntimeError> {
        match expr {
            Expr::Name(n) => self.resolve_type(n),
            Expr::Item { object, args } => {
                let base_id = self.resolve_type_expr(&object.node)?;
                let mut type_args = Vec::with_capacity(args.len());
                for a in args {
                    type_args.push(self.resolve_type_expr(&a.node)?);
                }
                match self.types.get(base_id) {
                    TypeDef::Enum { .. } => self
                        .types
                        .instantiate_enum(base_id, type_args)
                        .map_err(Into::into),
                    TypeDef::Struct { .. } => self
                        .types
                        .instantiate_struct(base_id, type_args)
                        .map_err(Into::into),
                    _ => Err(ErrorKind::WrongTypeKind {
                        type_id: base_id,
                        expected: TypeKindExpectation::GenericType,
                    }
                    .into()),
                }
            }
            _ => Err(ErrorKind::Unsupported("unsupported type annotation expression").into()),
        }
    }

    /// Check that a value conforms to an expected type.
    fn check_type(&self, value: &Value, expected: TypeId) -> Result<(), RuntimeError> {
        let actual = value.type_id();
        if actual != expected {
            return Err(ErrorKind::TypeMismatch { expected, actual }.into());
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
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::KindDef {
                name,
                type_params,
                fields,
            } => {
                self.register_struct(name, type_params, fields)
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::InterfaceDef {
                name,
                type_params,
                methods,
            } => {
                self.register_interface(name, type_params, methods)
                    .map_err(|e| e.at(stmt.span))?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::Impl {
                type_name,
                type_params,
                as_type,
                methods,
            } => {
                self.register_impl(type_name, type_params, methods, out)
                    .map_err(|e| e.at(stmt.span))?;
                if let Some(iface_expr) = as_type {
                    self.check_conformance(type_name, &iface_expr.node)
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
                        let type_id = self.resolve_type(type_name).map_err(|e| e.at(stmt.span))?;
                        if name == "Drop" {
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

                let func = Value::Func {
                    params: func_params,
                    ret_type: ret_texpr,
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
                let n: BigInt = s.parse().map_err(|e: num_bigint::ParseBigIntError| {
                    RuntimeError::new(ErrorKind::InvalidLiteral {
                        kind: "integer",
                        text: s.clone(),
                        reason: e.to_string(),
                    })
                    .at(expr.span)
                })?;
                Ok(Flow::Next(Value::Int(n)))
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
            Expr::Str(s) => Ok(Flow::Next(Value::Str(s.clone()))),

            Expr::Interp { parts } => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        InterpPart::Lit(s) => result.push_str(s),
                        InterpPart::Expr(inner) => {
                            let flow = self.eval_expr(inner, out)?;
                            let val = match flow {
                                Flow::Next(v) => v,
                                Flow::Return(v) => return Ok(Flow::Return(v)),
                                Flow::Break => return Ok(Flow::Break),
                                Flow::Continue => return Ok(Flow::Continue),
                            };
                            result.push_str(&val.display(&self.types));
                        }
                    }
                }
                Ok(Flow::Next(Value::Str(result)))
            }

            Expr::Name(name) => self.get(name).cloned().map(Flow::Next).ok_or_else(|| {
                RuntimeError::new(ErrorKind::Undefined {
                    kind: NameKind::Variable,
                    name: name.clone(),
                })
                .at(expr.span)
            }),

            Expr::With { bindings, body } => {
                self.push_scope();
                for (name, val_expr) in bindings {
                    let val = self.eval_value(val_expr, out)?;
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

            Expr::Attr { object, name } => {
                let obj = self.eval_value(object, out)?;
                self.eval_attr(&obj, name)
                    .map_err(|e: RuntimeError| e.at(expr.span))
            }

            Expr::Item { object, args } => {
                let obj = self.eval_value(object, out)?;
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
                }
                self.eval_item(&obj, &arg_vals)
                    .map_err(|e: RuntimeError| e.at(expr.span))
            }

            Expr::Call { callee, args } => {
                // Evaluate args once.
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
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
                    .map_err(|e: RuntimeError| e.at(expr.span))?;

                // Copy-in copy-out: write mutated self back to the receiver variable.
                if let Some(var_name) = receiver_var {
                    if let Some(mutated_self) = self.last_method_self.take() {
                        // Copy-out: replacing self with its mutated version.
                        // Don't drop the old value — it's the same object, pre-mutation.
                        let _ = self
                            .update_in_scope(&var_name, mutated_self)
                            .map_err(|e: RuntimeError| e.at(expr.span))?;
                    }
                }

                Ok(result)
            }

            Expr::BinOp { op, left, right } => {
                let lv = self.eval_value(left, out)?;
                let rv = self.eval_value(right, out)?;
                let result = native::eval_binop(*op, &lv, &rv)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(result))
            }

            Expr::If {
                cond,
                then_body,
                else_body,
            } => {
                let cv = self.eval_value(cond, out)?;
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
                binding,
                iter_expr,
                body,
            } => {
                // 1. Evaluate the iterable expression.
                let iterable = self.eval_value(iter_expr, out)?;

                // 2. Call .to_iter() on it.
                let iter_val = self
                    .call_method(&iterable, METHOD_TO_ITER, &[], out)
                    .map_err(|e: RuntimeError| e.at(expr.span))?;

                // 3. Loop: call .next() on the iterator.
                // The iterator lives as a Rust local — no synthetic variable in scope.
                let mut iterator = iter_val;
                loop {
                    let bound = self
                        .resolve_method(&iterator, METHOD_NEXT)
                        .map_err(|e: RuntimeError| e.at(expr.span))?;
                    let next_result = match self
                        .eval_call(bound, &[], out)
                        .map_err(|e: RuntimeError| e.at(expr.span))?
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
                        return Err(RuntimeError::new(ErrorKind::IteratorProtocol(
                            "iterator .next() must return an Opt value",
                        ))
                        .at(expr.span));
                    };

                    if self.types.variant_name(*opt_tid, *variant_idx) == VARIANT_NONE {
                        break;
                    }

                    // 5. Extract the value from Some(val).
                    let val = fields.first().cloned().ok_or_else(|| {
                        RuntimeError::new(ErrorKind::IteratorProtocol("Opt.Some has no field"))
                            .at(expr.span)
                    })?;

                    // Bind loop variable and execute body.
                    self.push_scope();
                    self.set(binding.clone(), val);

                    let flow = self.exec_block(body, out)?;
                    if let Some(early) = self.dispatch_loop_flow(flow, out) {
                        return Ok(early);
                    }
                }

                Ok(Flow::Next(Value::Nil))
            }

            Expr::While { cond, body } => {
                loop {
                    let cv = self.eval_value(cond, out)?;
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
                let lv = self.eval_value(left, out)?;
                if !native::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = self.eval_value(right, out)?;
                Ok(Flow::Next(rv))
            }

            Expr::Or { left, right } => {
                let lv = self.eval_value(left, out)?;
                if native::truth(&lv) {
                    return Ok(Flow::Next(lv));
                }
                let rv = self.eval_value(right, out)?;
                Ok(Flow::Next(rv))
            }

            Expr::UnaryOp { op, operand } => {
                let val = self.eval_value(operand, out)?;
                let result = native::eval_unaryop(*op, &val)
                    .map_err(|e| RuntimeError::from(e).at(expr.span))?;
                Ok(Flow::Next(result))
            }

            Expr::Construct { type_expr, fields } => {
                let type_val = self.eval_value(type_expr, out)?;
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

                // Check for extra fields.
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

                // Check for missing fields and build in definition order.
                let mut provided: IndexMap<String, Value> = IndexMap::new();
                for (fname, fexpr) in fields {
                    let val = self.eval_value(fexpr, out)?;
                    provided.insert(fname.clone(), val);
                }

                let mut result_fields = IndexMap::new();
                for (fname, expected_tid) in &expected_fields {
                    let val = provided.shift_remove(fname.as_str()).ok_or_else(|| {
                        RuntimeError::new(ErrorKind::MissingField {
                            type_id,
                            field: fname.clone(),
                        })
                        .at(expr.span)
                    })?;
                    self.check_type(&val, *expected_tid)
                        .map_err(|e| e.at(expr.span))?;
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
            Flow::Break => Err(RuntimeError::new(ErrorKind::FlowMisuse(
                FlowMisuse::BreakOutsideLoop,
            ))
            .at(expr.span)),
            Flow::Continue => Err(RuntimeError::new(ErrorKind::FlowMisuse(
                FlowMisuse::ContinueOutsideLoop,
            ))
            .at(expr.span)),
        }
    }

    // ── Enum construction ────────────────────────────────────────────────

    fn register_enum(
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
            variants.insert(v.name.clone(), VariantDef { fields });
        }

        let type_id = self
            .types
            .register_enum(name.to_string(), type_params.to_vec(), variants);

        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
    }

    // ── Struct registration ────────────────────────────────────────────

    fn register_struct(
        &mut self,
        name: &str,
        type_params: &[String],
        ast_fields: &[AstFieldDef],
    ) -> Result<(), RuntimeError> {
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
    ) -> Result<(), RuntimeError> {
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
    fn check_conformance(&self, type_name: &str, iface_expr: &Expr) -> Result<(), RuntimeError> {
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

        let type_id = self.resolve_type(type_name)?;
        let method_table = self.methods.get(&type_id).ok_or_else(|| -> RuntimeError {
            ErrorKind::ConformanceFailure {
                type_name: type_name.to_string(),
                iface_name: iface_name.to_string(),
                detail: ConformanceError::TypeHasNoMethods,
            }
            .into()
        })?;

        for sig in &iface.methods {
            let func = method_table.get(&sig.name).ok_or_else(|| -> RuntimeError {
                ErrorKind::ConformanceFailure {
                    type_name: type_name.to_string(),
                    iface_name: iface_name.to_string(),
                    detail: ConformanceError::MissingMethod {
                        method: sig.name.clone(),
                    },
                }
                .into()
            })?;

            if let Value::Func { params, .. } = func {
                if params.len() != sig.params.len() {
                    return Err(ErrorKind::ConformanceFailure {
                        type_name: type_name.to_string(),
                        iface_name: iface_name.to_string(),
                        detail: ConformanceError::ParamCountMismatch {
                            method: sig.name.clone(),
                            expected: sig.params.len(),
                            actual: params.len(),
                        },
                    }
                    .into());
                }
            }
        }

        Ok(())
    }

    // ── Impl registration ────────────────────────────────────────────

    fn register_impl(
        &mut self,
        type_name: &str,
        type_params: &[String],
        methods: &[Spanned<FuncDef>],
        _out: &mut impl Write,
    ) -> Result<(), RuntimeError> {
        let type_id = self.resolve_type(type_name)?;

        // Make `Self` available as a type alias within this impl block.
        self.set("Self".into(), Value::Type(type_id));

        for method in methods {
            let FuncDef {
                name,
                params,
                ret_type,
                body,
            } = &method.node;

            if params.is_empty() || params[0].name != SELF_PARAM {
                return Err(ErrorKind::MethodDef {
                    method: name.clone(),
                    detail: MethodDefError::MissingSelf,
                }
                .into());
            }

            // Resolve params using type_params so generic annotations
            // (e.g., `val: T`) produce TypeExpr::Param(idx).
            let func_params = self.resolve_params_with_type_params(params, type_params)?;

            let ret_texpr = ret_type
                .as_ref()
                .map(|ann| self.resolve_type_ann(&ann.node, type_params))
                .transpose()?;

            let func = Value::Func {
                params: func_params,
                ret_type: ret_texpr,
                body: body.clone(),
            };

            self.methods
                .entry(type_id)
                .or_insert_with(IndexMap::new)
                .insert(name.clone(), func);
        }

        // Remove `Self` so it doesn't leak into surrounding scope.
        self.remove("Self");

        Ok(())
    }

    /// Convert an expression used as a type annotation to a TypeExpr.
    /// Handles bare names (`Int`, `T`), and generic applications (`Ptr[T]`, `Res[T, E]`).
    fn resolve_type_ann(
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
                            ErrorKind::Unsupported("nested generic base must be a name").into()
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

    // ── Attr assignment: a.b = v ─────────────────────────────────────

    fn exec_attr_assign(
        &mut self,
        object: &Spanned<Expr>,
        attr: &str,
        val: Value,
    ) -> Result<Flow, RuntimeError> {
        let Expr::Name(var_name) = &object.node else {
            return Err(ErrorKind::Unsupported("nested attr assignment not yet supported").into());
        };

        let (type_id, expected_tid) = {
            let current = self.get(var_name).ok_or_else(|| -> RuntimeError {
                ErrorKind::Undefined {
                    kind: NameKind::Variable,
                    name: var_name.clone(),
                }
                .into()
            })?;
            let Value::Struct { type_id, .. } = current else {
                return Err(ErrorKind::NoAttr {
                    type_id: current.type_id(),
                    attr: attr.to_string(),
                    access: AccessKind::Field,
                }
                .into());
            };
            let struct_fields = self
                .types
                .get_struct_fields(*type_id)
                .map_err(RuntimeError::from)?;
            let expected_tid =
                struct_fields
                    .get(attr)
                    .copied()
                    .ok_or_else(|| -> RuntimeError {
                        ErrorKind::NoAttr {
                            type_id: *type_id,
                            attr: attr.to_string(),
                            access: AccessKind::Field,
                        }
                        .into()
                    })?;
            (*type_id, expected_tid)
        };

        self.check_type(&val, expected_tid)?;

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
        Err(ErrorKind::Undefined {
            kind: NameKind::Variable,
            name: var_name.clone(),
        }
        .into())
    }

    // ── Method helpers ────────────────────────────────────────────────

    /// Look up a method by name. Falls back from instance to base type
    /// (e.g., Buf[Int] → Buf) so generic methods work.
    fn lookup_method(&self, type_id: TypeId, name: &str) -> Option<Value> {
        // Try exact type first.
        if let Some(method) = self.methods.get(&type_id).and_then(|t| t.get(name)) {
            return Some(method.clone());
        }
        // Fall back to base type for instances.
        let base = self.types.base_type(type_id);
        if base != type_id {
            return self.methods.get(&base).and_then(|t| t.get(name)).cloned();
        }
        None
    }

    /// Wrap a Func value as a BoundMethod with the given receiver.
    fn bind_method(
        &self,
        receiver: Value,
        method: Value,
        _name: &str,
    ) -> Result<Value, RuntimeError> {
        if !matches!(method, Value::Func { .. }) {
            return Err(ErrorKind::Other("bound method does not wrap a Func".to_string()).into());
        }
        Ok(Value::BoundMethod {
            receiver: Box::new(receiver),
            func: Box::new(method),
        })
    }

    /// Look up and bind a method, ready to call.
    fn resolve_method(&self, receiver: &Value, name: &str) -> Result<Value, RuntimeError> {
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
    fn call_method(
        &mut self,
        receiver: &Value,
        name: &str,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        let bound = self.resolve_method(receiver, name)?;
        match self.eval_call(bound, args, out)? {
            Flow::Next(v) | Flow::Return(v) => Ok(v),
            _ => Err(ErrorKind::Other(format!("method '{name}' returned abnormal flow")).into()),
        }
    }

    // ── Attr: a.b ─────────────────────────────────────────────────────

    fn eval_attr(&self, object: &Value, name: &str) -> Result<Flow, RuntimeError> {
        match object {
            Value::Type(type_id) => {
                let def = self.types.get(*type_id);
                match def {
                    TypeDef::EnumInstance { variants, .. } => {
                        let (idx, _, vdef) =
                            variants.get_full(name).ok_or_else(|| -> RuntimeError {
                                ErrorKind::NoAttr {
                                    type_id: *type_id,
                                    attr: name.to_string(),
                                    access: AccessKind::Variant,
                                }
                                .into()
                            })?;
                        let variant_idx = idx as u32;

                        if vdef.fields.is_empty() {
                            Ok(Flow::Next(Value::Enum {
                                type_id: *type_id,
                                variant_idx,
                                fields: vec![],
                            }))
                        } else {
                            Ok(Flow::Next(Value::VariantConstructor {
                                type_id: *type_id,
                                variant_idx,
                                field_types: vdef.fields.clone(),
                            }))
                        }
                    }
                    TypeDef::StructInstance { .. } => Err(ErrorKind::WrongTypeKind {
                        type_id: *type_id,
                        expected: TypeKindExpectation::ExpectedEnum,
                    }
                    .into()),
                    _ => Err(ErrorKind::NoAttr {
                        type_id: *type_id,
                        attr: name.to_string(),
                        access: AccessKind::Attr,
                    }
                    .into()),
                }
            }
            Value::Struct { type_id, fields } => {
                if let Some(val) = fields.get(name) {
                    return Ok(Flow::Next(val.clone()));
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
            Value::Module(module_id) => {
                let module = self.native_registry.get_module(*module_id);
                match module.entries.get(name) {
                    Some(ModuleItem::SubModule(sub_id)) => Ok(Flow::Next(Value::Module(*sub_id))),
                    Some(ModuleItem::NativeFn(fn_id)) => Ok(Flow::Next(Value::NativeFn(*fn_id))),
                    None => Err(ErrorKind::NoAttr {
                        type_id: prim::NIL,
                        attr: name.to_string(),
                        access: AccessKind::Attr,
                    }
                    .into()),
                }
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

    fn eval_item(&mut self, object: &Value, args: &[Value]) -> Result<Flow, RuntimeError> {
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
                let instance_id = match self.types.get(*base_id) {
                    TypeDef::Enum { .. } => self
                        .types
                        .instantiate_enum(*base_id, type_args)
                        .map_err(RuntimeError::from)?,
                    TypeDef::Struct { .. } => self
                        .types
                        .instantiate_struct(*base_id, type_args)
                        .map_err(RuntimeError::from)?,
                    _ => {
                        return Err(ErrorKind::WrongTypeKind {
                            type_id: *base_id,
                            expected: TypeKindExpectation::GenericType,
                        }
                        .into())
                    }
                };
                Ok(Flow::Next(Value::Type(instance_id)))
            }
            other => Err(ErrorKind::WrongTypeKind {
                type_id: other.type_id(),
                expected: TypeKindExpectation::Indexable,
            }
            .into()),
        }
    }

    // ── Call: a(b) ───────────────────────────────────────────────────────

    fn eval_call(
        &mut self,
        func: Value,
        args: &[Value],
        out: &mut impl Write,
    ) -> Result<Flow, RuntimeError> {
        match func {
            Value::Func {
                params,
                ret_type,
                body,
            } => {
                if args.len() != params.len() {
                    return Err(ErrorKind::ArityMismatch {
                        target: ArityTarget::Function,
                        expected: params.len(),
                        actual: args.len(),
                    }
                    .into());
                }

                let result =
                    self.call_func_body(&params, args, &ret_type, &body, false, &[], out)?;
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
                    return Err(
                        ErrorKind::Other("bound method does not wrap a Func".to_string()).into(),
                    );
                };

                let method_params = &params[1..];
                if args.len() != method_params.len() {
                    return Err(ErrorKind::ArityMismatch {
                        target: ArityTarget::Method,
                        expected: method_params.len(),
                        actual: args.len(),
                    }
                    .into());
                }

                let mut full_args = Vec::with_capacity(params.len());
                let receiver_type_args = self.types.instance_type_args(receiver.type_id());
                full_args.push(*receiver);
                full_args.extend_from_slice(args);

                let result = self.call_func_body(
                    &params,
                    &full_args,
                    &ret_type,
                    &body,
                    true,
                    &receiver_type_args,
                    out,
                )?;
                Ok(Flow::Next(result))
            }

            Value::NativeFn(fn_id) => {
                let entry = self.native_registry.get(fn_id);
                if entry.requires_unsafe && !self.in_unsafe {
                    return Err(ErrorKind::UnsafeRequired {
                        intrinsic: entry.name.to_string(),
                    }
                    .into());
                }
                let handler = entry.handler;
                let mut ctx = NativeCtx {
                    types: &self.types,
                    allocations: &mut self.allocations,
                    out,
                    in_unsafe: self.in_unsafe,
                };
                let result = handler(&mut ctx, args)?;
                Ok(Flow::Next(result))
            }

            other => Err(ErrorKind::WrongTypeKind {
                type_id: other.type_id(),
                expected: TypeKindExpectation::Callable,
            }
            .into()),
        }
    }

    // ── Shared helpers ──────────────────────────────────────────────────

    /// Resolve AST params to FuncParam values (no generic type params).
    fn resolve_params(&mut self, params: &[Param]) -> Result<Vec<FuncParam>, RuntimeError> {
        self.resolve_params_with_type_params(params, &[])
    }

    /// Resolve AST params to FuncParam values, with optional generic type params.
    fn resolve_params_with_type_params(
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
                    name: p.name.clone(),
                    type_ann,
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
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::RetOutsideFunction {
                            context: context.to_string(),
                        },
                    ))
                    .at(stmt.span))
                }
                Flow::Break => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::BreakOutsideLoop,
                    ))
                    .at(stmt.span))
                }
                Flow::Continue => {
                    return Err(RuntimeError::new(ErrorKind::FlowMisuse(
                        FlowMisuse::ContinueOutsideLoop,
                    ))
                    .at(stmt.span))
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
        ret_type: &Option<TypeExpr>,
        body: &[Spanned<Stmt>],
        is_method: bool,
        instance_type_args: &[TypeId],
        out: &mut impl Write,
    ) -> Result<Value, RuntimeError> {
        for (param, val) in params.iter().zip(args.iter()) {
            if let Some(ref texpr) = param.type_ann {
                let expected = self
                    .types
                    .resolve_texpr(texpr.clone(), instance_type_args)?;
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
                self.pop_scope(out);
                return Err(ErrorKind::FlowMisuse(FlowMisuse::BreakOutsideLoop).into());
            }
            Ok(Flow::Continue) => {
                self.pop_scope(out);
                return Err(ErrorKind::FlowMisuse(FlowMisuse::ContinueOutsideLoop).into());
            }
            Err(e) => {
                self.pop_scope(out);
                return Err(e);
            }
        };

        if is_method {
            self.last_method_self = self.get(SELF_PARAM).cloned();
            // Remove self from the frame before popping so it doesn't get
            // dropped — the caller owns the original, not this copy.
            self.remove(SELF_PARAM);
        }

        self.pop_scope(out);

        if let Some(ref ret_texpr) = ret_type {
            let expected_ret = self
                .types
                .resolve_texpr(ret_texpr.clone(), instance_type_args)?;
            self.check_type(&result, expected_ret)?;
        }

        Ok(result)
    }

    /// Dispatch loop-body flow control. Always pops scope.
    /// Returns `None` to continue looping, `Some(flow)` to exit.
    fn dispatch_loop_flow(&mut self, flow: Flow, out: &mut impl Write) -> Option<Flow> {
        match flow {
            Flow::Next(_) | Flow::Continue => {
                self.pop_scope(out);
                None
            }
            Flow::Break => {
                self.pop_scope(out);
                Some(Flow::Next(Value::Nil))
            }
            ret @ Flow::Return(_) => {
                self.pop_scope(out);
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
            matches!(
                err.kind,
                ErrorKind::Undefined {
                    kind: NameKind::Variable,
                    ..
                }
            ),
            "unexpected error: {:?}",
            err.kind
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
            matches!(
                err.kind,
                ErrorKind::FlowMisuse(FlowMisuse::RetOutsideFunction { .. })
            ),
            "unexpected error: {:?}",
            err.kind
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
