use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

use indexmap::IndexMap;
use num_bigint::BigInt;
use tracing::{debug, trace};

use super::ast::{
    AssignTarget, AstFieldDef, AstVariantDef, BinInterpPart, Expr, FuncDef, InterpPart, MatchArm,
    MethodSig, Param, Pattern, Program, Span, Spanned, Stmt, TypePattern,
};
use super::error::{
    AccessKind, ArityTarget, ConformanceError, ErrorKind, FlowMisuse, MethodDefError, NameKind,
    PatternKind, RuntimeError, TypeKindExpectation,
};
use super::native::{self, NativeCtx, NativeFnRegistry};
use super::types::{prim, TypeDef, TypeExpr, TypeId, TypeRegistry, VariantDef};
use super::value::{FuncData, FuncParam, Value};

// Submodules.
mod access;
mod call;
mod imports;
mod match_;
mod registration;
mod types_protocol;
mod types_resolve;

pub use types_protocol::{Flow, Protocol};
use types_protocol::{
    eval, parse_int_literal, INTERFACE_DROP, SELF_PARAM, VARIANT_NONE, VARIANT_VAL,
};

// ── Interface storage ────────────────────────────────────────────

/// A registered interface — stores method signatures for conformance checking.
#[derive(Debug, Clone)]
pub(super) struct InterfaceDef {
    #[allow(dead_code)]
    pub(super) type_params: Vec<String>,
    pub(super) methods: Vec<MethodSig>,
}

// ── Interpreter ──────────────────────────────────────────────────────────────

/// The KataScript interpreter. Owns the type registry, variable scopes,
/// and all evaluation logic.
pub struct Interpreter {
    /// All registered types.
    pub types: TypeRegistry,
    /// Mutable call stack — live execution frames.
    call_stack: Vec<super::scope::Frame>,
    /// Frozen closure scope — base for lookup after call_stack is exhausted.
    closure_scope: Option<Arc<super::scope::Scope>>,
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
    /// Embedded standard library modules: "mem" → source code.
    std_modules: HashMap<String, &'static str>,
    /// Modules already loaded: key → ModuleId (prevent double-loading).
    loaded_modules: HashMap<String, native::ModuleId>,
    /// Native function registry and module tree.
    native_registry: NativeFnRegistry,
    /// Conformance registry: (concrete_base_type, interface_base_type) pairs.
    /// Populated during `impl K as T`. Queried for `as` and implicit coercion.
    conformances: HashSet<(TypeId, TypeId)>,
    /// Interning tables for immutable shared data (strings, bins, ints).
    intern: super::intern::InternTables,
}

impl Interpreter {
    /// Create a new interpreter with primitive types bootstrapped.
    pub fn new() -> Self {
        let types = TypeRegistry::new();
        // Bootstrap native functions and module tree.
        let boot = native::bootstrap();

        let mut std_modules = HashMap::new();
        // core and sub-modules
        std_modules.insert("core".into(), include_str!("../../../../std/core/mod.ks"));
        std_modules.insert("core.opt".into(), include_str!("../../../../std/core/opt.ks"));
        std_modules.insert("core.res".into(), include_str!("../../../../std/core/res.ks"));
        std_modules.insert(
            "core.iter".into(),
            include_str!("../../../../std/core/iter.ks"),
        );
        std_modules.insert(
            "core.lifecycle".into(),
            include_str!("../../../../std/core/lifecycle.ks"),
        );
        std_modules.insert(
            "core.indexing".into(),
            include_str!("../../../../std/core/indexing.ks"),
        );
        std_modules.insert(
            "core.conv".into(),
            include_str!("../../../../std/core/conv.ks"),
        );
        // mem and sub-modules
        std_modules.insert("mem".into(), include_str!("../../../../std/mem/mod.ks"));
        std_modules.insert(
            "mem.allocator".into(),
            include_str!("../../../../std/mem/allocator.ks"),
        );
        std_modules.insert("mem.ptr".into(), include_str!("../../../../std/mem/ptr.ks"));
        std_modules.insert("mem.buf".into(), include_str!("../../../../std/mem/buf.ks"));
        // dsa and sub-modules
        std_modules.insert("dsa".into(), include_str!("../../../../std/dsa/mod.ks"));
        std_modules.insert("dsa.arr".into(), include_str!("../../../../std/dsa/arr.ks"));
        std_modules.insert("dsa.map".into(), include_str!("../../../../std/dsa/map.ks"));

        let mut interp = Self {
            types,
            call_stack: vec![super::scope::Frame::new()],
            closure_scope: None,
            methods: HashMap::new(),
            interfaces: IndexMap::new(),
            last_method_self: None,
            std_modules,
            loaded_modules: HashMap::new(),
            drop_types: HashSet::new(),
            dropping: false,
            in_unsafe: false,
            allocations: Vec::new(),
            native_registry: boot.registry,
            conformances: HashSet::new(),
            intern: super::intern::InternTables::new(),
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
        interp.set("RawPtr".into(), Value::Type(prim::RAW_PTR));
        interp.set("Byte".into(), Value::Type(prim::BYTE));
        interp.set("Char".into(), Value::Type(prim::CHAR));
        interp.set("Tup".into(), Value::Type(prim::TUPLE));

        // Top-level native functions.
        interp.set("print".into(), Value::NativeFn(boot.print_id));
        interp.set("typeof".into(), Value::NativeFn(boot.typeof_id));
        interp.set("panic".into(), Value::NativeFn(boot.panic_id));

        // Top-level native modules: `ops.*`, `mem.*`.
        interp.set("ops".into(), Value::Module(boot.ops_module));
        interp.set("mem".into(), Value::Module(boot.mem_module));

        // Native methods for prim types.
        for (name, fn_id) in &boot.int_methods.methods {
            interp
                .methods
                .entry(prim::INT)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.float_methods.methods {
            interp
                .methods
                .entry(prim::FLOAT)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.bool_methods.methods {
            interp
                .methods
                .entry(prim::BOOL)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.byte_methods.methods {
            interp
                .methods
                .entry(prim::BYTE)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.char_methods.methods {
            interp
                .methods
                .entry(prim::CHAR)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.str_methods.methods {
            interp
                .methods
                .entry(prim::STR)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }
        for (name, fn_id) in &boot.bin_methods.methods {
            interp
                .methods
                .entry(prim::BIN)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }

        for (name, fn_id) in &boot.tup_methods.methods {
            interp
                .methods
                .entry(prim::TUPLE)
                .or_insert_with(IndexMap::new)
                .insert(name.to_string(), Value::NativeFn(*fn_id));
        }

        // Fixed-width numeric types.
        for (name, tid) in super::numeric::type_entries() {
            interp.set((*name).into(), Value::Type(*tid));
        }
        for (tid, methods) in &boot.numeric_methods {
            for (name, fn_id) in &methods.methods {
                interp
                    .methods
                    .entry(*tid)
                    .or_insert_with(IndexMap::new)
                    .insert(name.to_string(), Value::NativeFn(*fn_id));
            }
        }

        interp
    }

    /// Access the type registry (for formatting error messages, etc.).
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.types
    }

    /// Intern a byte sequence, returning a shared `Bin` value.
    pub fn intern_bin(&mut self, bytes: Vec<u8>) -> Value {
        Value::Bin(self.intern.intern_bin(bytes))
    }

    /// All names visible in the current scope (for REPL completion).
    pub fn visible_names(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut names = Vec::new();
        for frame in self.call_stack.iter().rev() {
            for key in frame.keys() {
                if seen.insert(key.clone()) {
                    names.push(key.clone());
                }
            }
        }
        names.sort();
        names
    }

    // ── Completion (REPL) ─────────────────────────────────────────────

    /// Evaluate an expression safely for tab completion. Only handles
    /// side-effect-free forms: names, attribute access, type instantiation.
    /// Returns None for anything that could have side effects.
    fn eval_for_completion(&mut self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Name(n) => self.get(n).cloned(),
            Expr::Attr { object, name, .. } => {
                let obj = self.eval_for_completion(&object.node)?;
                match self.eval_attr(&obj, name) {
                    Ok(Flow::Next(v)) => Some(v),
                    _ => None,
                }
            }
            Expr::Item { object, args } => {
                let obj = self.eval_for_completion(&object.node)?;
                let Value::Type(base_id) = obj else {
                    return None;
                };
                let type_args: Vec<TypeId> = args
                    .iter()
                    .map(|a| match self.eval_for_completion(&a.node)? {
                        Value::Type(tid) => Some(tid),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let instance_id = match self.types.get(base_id) {
                    TypeDef::Enum { .. } => self.types.instantiate_enum(base_id, type_args).ok()?,
                    TypeDef::Struct { .. } => {
                        self.types.instantiate_struct(base_id, type_args).ok()?
                    }
                    TypeDef::Interface { .. } => {
                        self.types.instantiate_interface(base_id, type_args).ok()?
                    }
                    _ => return None,
                };
                Some(Value::Type(instance_id))
            }
            _ => None,
        }
    }

    /// Get attribute completions for a value (fields, methods, variants, entries).
    fn completions_for_value(&self, val: &Value) -> Vec<String> {
        let mut names = Vec::new();
        match val {
            Value::Module(mid) => {
                let module = self.native_registry.get_module(*mid);
                names.extend(module.entries.keys().cloned());
            }
            Value::Rec { type_id, .. } | Value::Tup { type_id, .. } => {
                if let Some(field_names) = self.types.field_names(*type_id) {
                    names.extend(field_names.iter().map(|s| s.to_string()));
                }
                self.collect_methods(*type_id, &mut names);
            }
            Value::Type(type_id) => {
                if let TypeDef::EnumInstance { variants, .. } = self.types.get(*type_id) {
                    names.extend(variants.keys().cloned());
                }
                self.collect_methods(*type_id, &mut names);
            }
            other => {
                self.collect_methods(other.type_id(), &mut names);
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// Collect method names for a type (including base-type fallback).
    fn collect_methods(&self, type_id: TypeId, names: &mut Vec<String>) {
        if let Some(methods) = self.methods.get(&type_id) {
            names.extend(methods.keys().cloned());
        }
        let base = self.types.base_type(type_id);
        if base != type_id {
            if let Some(methods) = self.methods.get(&base) {
                names.extend(methods.keys().cloned());
            }
        }
    }

    /// Parse and safely evaluate an expression for REPL tab completion.
    /// Returns attribute completions for the result, or empty if unsafe/invalid.
    pub fn completions_for_expr(&mut self, receiver_src: &str) -> Vec<String> {
        let source = format!("{receiver_src};");
        let Ok(program) = super::parser::parse_silent(&source) else {
            return Vec::new();
        };
        let expr = match program.first() {
            Some(spanned) => match &spanned.node {
                Stmt::Expr(e) => e,
                _ => return Vec::new(),
            },
            None => return Vec::new(),
        };
        let Some(val) = self.eval_for_completion(&expr.node) else {
            return Vec::new();
        };
        self.completions_for_value(&val)
    }

    // ── Scope ────────────────────────────────────────────────────────────

    fn get(&self, name: &str) -> Option<&Value> {
        // Walk mutable call stack (innermost first)
        for frame in self.call_stack.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        // Walk frozen closure scope chain
        self.closure_scope.as_ref().and_then(|s| s.lookup(name))
    }

    fn set(&mut self, name: String, value: Value) {
        self.call_stack
            .last_mut()
            .expect("interpreter always has at least one frame")
            .set(name, value);
    }

    /// Remove a name from the current (innermost) scope frame.
    fn remove(&mut self, name: &str) {
        if let Some(frame) = self.call_stack.last_mut() {
            frame.remove(name);
        }
    }

    fn push_scope(&mut self) {
        self.call_stack.push(super::scope::Frame::new());
    }

    /// Freeze the current scope for closure capture.
    /// Returns an Arc<Scope> representing everything visible now.
    fn capture_scope(&self) -> Arc<super::scope::Scope> {
        // Build chain: innermost call_stack frame → ... → outermost → closure_scope.
        // call_stack[last] is innermost, so we iterate forward and nest.
        let mut scope = self.closure_scope.clone();
        for frame in self.call_stack.iter() {
            scope = Some(Arc::new(super::scope::Scope {
                frame: frame.clone(),
                parent: scope,
            }));
        }
        scope.expect("capture_scope called with no scope")
    }

    fn pop_scope(&mut self, out: &mut impl Write) {
        debug_assert!(self.call_stack.len() > 1, "cannot pop the global frame");
        let frame = self.call_stack.pop().unwrap();
        if !self.dropping {
            for (_name, value) in frame.drain() {
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
            let _ = self.call_method(&value, Protocol::Drop.method_name(), &[], out);
            self.dropping = false;
        }
        // Recursively drop struct fields.
        if let Value::Rec { fields, .. } | Value::Tup { fields, .. } = &value {
            for field_val in fields.iter() {
                self.drop_value(field_val.clone(), out);
            }
        }
    }

    /// Update an existing variable in the nearest enclosing scope that contains it.
    /// Returns the old value (if any) for drop dispatch by the caller.
    fn update_in_scope(&mut self, name: &str, value: Value) -> Result<Option<Value>, RuntimeError> {
        // Update in the mutable call stack.
        for frame in self.call_stack.iter_mut().rev() {
            if frame.contains(name) {
                let old = frame.set(name.to_string(), value);
                return Ok(old);
            }
        }
        // If the variable exists in the frozen closure scope, the update is
        // a no-op — closures capture values, not mutable references.
        // This handles copy-out on captured variables (e.g., heap.grow()
        // tries to write back to `heap` which lives in a closure).
        if self
            .closure_scope
            .as_ref()
            .and_then(|s| s.lookup(name))
            .is_some()
        {
            return Ok(None);
        }
        Err(ErrorKind::Undefined {
            kind: NameKind::Variable,
            name: name.to_string(),
        }
        .into())
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
            self.exec_top_level(pre, out)?;
        }

        debug!(stmts = program.len(), "exec_program");
        self.exec_top_level(program, out)?;
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


    // ── Array literal ────────────────────────────────────────────────────

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
                flow @ (Flow::Return { .. }
                | Flow::Propagate { .. }
                | Flow::Bail(_)
                | Flow::Cont(_)) => return Ok(flow),
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
                    super::intern::InternTables::make_str(result),
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

            Expr::Name(name) => self.get(name).cloned().map(Flow::Next).ok_or_else(|| {
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
                    if self.types.try_shape(type_id).is_some() {
                        let variant_name = self.types.variant_name(type_id, variant_idx);
                        if variant_name == VARIANT_VAL {
                            if let Some(inner_val) = fields.first() {
                                return Ok(Flow::Next(inner_val.clone()));
                            }
                        } else {
                            // "Non" (Opt) or "Err" (Res) — propagate upward
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
                    if self.types.try_shape(type_id).is_some() {
                        let variant_name = self.types.variant_name(type_id, variant_idx);
                        if variant_name == VARIANT_VAL {
                            if let Some(inner_val) = fields.first() {
                                return Ok(Flow::Next(inner_val.clone()));
                            }
                        } else {
                            // "Non" (Opt) or "Err" (Res) — panic
                            return Err(RuntimeError::new(ErrorKind::UnwrapFailed {
                                type_id,
                                variant: variant_name.to_string(),
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
                    // Already the right type — no wrapping needed.
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
                                let iface = *interface_id;
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
                        // If the variable holds an AsType, re-wrap after copy-out.
                        let write_back =
                            if let Some(Value::AsType { interface_id, .. }) = self.get(&var_name) {
                                let iface = *interface_id;
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
                        // Type mismatch: label each operand, no primary span.
                        let left_type = self.types.display_name(lv.type_id());
                        let right_type = self.types.display_name(rv.type_id());
                        err = err
                            .at(left.span) // primary on left operand for ariadne line ref
                            .label(left.span, left_type.clone())
                            .label(right.span, right_type.clone());
                        if left_type != right_type {
                            err = err.help(format!(
                                "'{sym}' works on matching types: Int{sym}Int, Float{sym}Float, or Str{sym}Str",
                                sym = op.symbol()
                            ));
                        }
                    } else {
                        // Value errors (div by zero, NaN): span the full expression.
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
                // 0. Validate the loop pattern once (no duplicate bindings).
                self.check_unique_bindings(pattern)?;

                // 1. Evaluate the iterable expression.
                let iterable = eval!(self, iter_expr, out);

                // 2. Call .to_iter() on it.
                let iter_val = self
                    .call_method(&iterable, Protocol::ToIter.method_name(), &[], out)
                    .map_err(|e: RuntimeError| e.at(expr.span))?;

                // 3. Loop: call .next() on the iterator.
                // The iterator lives as a Rust local — no synthetic variable in scope.
                let mut iterator = iter_val;
                loop {
                    let bound = self
                        .resolve_method(&iterator, Protocol::Next.method_name())
                        .map_err(|e: RuntimeError| e.at(expr.span))?;
                    let next_result = match self
                        .eval_call(bound, &[], &[], out)
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

                    // Destructure the yielded value with the loop pattern, bind
                    // the resulting variables in a fresh scope, and execute the body.
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

                // Evaluate field values, keeping spans for error reporting.
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

    /// Execute a function body: type-check args, push scope, bind params,
    /// run body, pop scope, type-check return value.
    ///
    /// When `is_method` is true, stashes the final value of `self` in
    /// `last_method_self` before popping the scope (copy-out semantics).

    /// Dispatch loop-body flow control. Always pops scope.
    /// Returns `None` to continue looping, `Some(flow)` to exit.
    fn dispatch_loop_flow(&mut self, flow: Flow, out: &mut impl Write) -> Option<Flow> {
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
            args_span: (0, 0),
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
        let prog = program(vec![s(Stmt::Ret {
            keyword: (0, 3),
            value: s(Expr::Int("42".into())),
        })]);
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
                args_span: (0, 0),
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
