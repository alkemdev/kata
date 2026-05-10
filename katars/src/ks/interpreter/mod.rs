use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

use indexmap::IndexMap;

use super::ast::{Expr, MethodSig, Stmt};
use super::error::{ErrorKind, NameKind, RuntimeError};
use super::native::{self, NativeFnRegistry};
use super::types::{prim, TypeDef, TypeId, TypeRegistry};
use super::value::Value;

// Submodules.
mod access;
mod call;
mod expr;
mod imports;
mod match_;
mod method_id;
mod registration;
mod stmt;
mod types_protocol;
mod types_resolve;

pub use method_id::{MethodId, MethodInterner, ProtocolMethods};
pub use types_protocol::{Flow, Protocol};

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
    /// Method tables: TypeId → MethodId → Func value. Method names are
    /// interned in `method_interner`; lookup is a u32-keyed hash hit.
    methods: HashMap<TypeId, IndexMap<MethodId, Value>>,
    /// Bidirectional method-name ⇄ id table. Populated at registration
    /// time; used in lookup mode (read-only) on the dispatch hot path.
    method_interner: MethodInterner,
    /// Pre-interned `MethodId`s for the language-level protocols.
    protocol_methods: ProtocolMethods,
    /// Interface definitions: name → method signatures.
    interfaces: IndexMap<String, InterfaceDef>,
    /// Temporary: holds mutated `self` after a method call for copy-out.
    last_method_self: Option<Value>,
    /// TypeIds that implement the Drop protocol.
    drop_types: HashSet<TypeId>,
    /// Cached TypeId for the `Drop` interface (resolved once after the
    /// prelude registers it). `None` until then. Used to recognize
    /// `impl K as Drop { … }` blocks by handle, not by string name.
    drop_interface_id: Option<TypeId>,
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

        // Pre-intern protocol method names so runtime dispatch never
        // re-interns them.
        let mut method_interner = MethodInterner::new();
        let protocol_methods = ProtocolMethods::new(&mut method_interner);

        let mut interp = Self {
            types,
            call_stack: vec![super::scope::Frame::new()],
            closure_scope: None,
            methods: HashMap::new(),
            method_interner,
            protocol_methods,
            interfaces: IndexMap::new(),
            last_method_self: None,
            std_modules,
            loaded_modules: HashMap::new(),
            drop_types: HashSet::new(),
            drop_interface_id: None,
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

        // Native methods for prim types. Each name is interned through
        // `method_interner` so the methods table is keyed by `MethodId`.
        let prim_method_groups: [(TypeId, &super::native::PrimMethods); 8] = [
            (prim::INT, &boot.int_methods),
            (prim::FLOAT, &boot.float_methods),
            (prim::BOOL, &boot.bool_methods),
            (prim::BYTE, &boot.byte_methods),
            (prim::CHAR, &boot.char_methods),
            (prim::STR, &boot.str_methods),
            (prim::BIN, &boot.bin_methods),
            (prim::TUPLE, &boot.tup_methods),
        ];
        for (tid, methods) in prim_method_groups {
            interp.register_native_methods(tid, methods);
        }

        // Fixed-width numeric types (U8…U128, I8…I128, etc.).
        for (name, tid) in super::numeric::type_entries() {
            interp.set((*name).into(), Value::Type(*tid));
        }
        for (tid, methods) in &boot.numeric_methods {
            interp.register_native_methods(*tid, methods);
        }

        interp
    }

    /// Register a batch of native methods on a type. Interns each name in
    /// `method_interner` and inserts the resulting `MethodId → Value` pair
    /// into the methods table for `type_id`.
    fn register_native_methods(
        &mut self,
        type_id: TypeId,
        methods: &super::native::PrimMethods,
    ) {
        // Pre-intern in a separate pass so we don't double-borrow `self`.
        let interned: Vec<(MethodId, Value)> = methods
            .methods
            .iter()
            .map(|(name, fn_id)| (self.method_interner.intern(name), Value::NativeFn(*fn_id)))
            .collect();
        let table = self.methods.entry(type_id).or_insert_with(IndexMap::new);
        for (mid, val) in interned {
            table.insert(mid, val);
        }
    }

    /// Access the type registry (for formatting error messages, etc.).
    /// Cached TypeId for the `Drop` interface — looked up the first time
    /// it's needed (after the prelude registers it) and reused thereafter.
    /// Returns `None` if `Drop` isn't registered (impossible in normal
    /// runs, possible in unit tests that skip the prelude).
    pub(super) fn drop_interface_id(&mut self) -> Option<TypeId> {
        if self.drop_interface_id.is_none() {
            self.drop_interface_id = self.types.lookup("Drop");
        }
        self.drop_interface_id
    }

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
            Expr::Name(n) => self.get(n),
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
    /// Names are recovered from the interner for display.
    fn collect_methods(&self, type_id: TypeId, names: &mut Vec<String>) {
        if let Some(methods) = self.methods.get(&type_id) {
            for &mid in methods.keys() {
                names.push(self.method_interner.name(mid).to_string());
            }
        }
        let base = self.types.base_type(type_id);
        if base != type_id {
            if let Some(methods) = self.methods.get(&base) {
                for &mid in methods.keys() {
                    names.push(self.method_interner.name(mid).to_string());
                }
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

    /// Read a variable's value. Walks the live call stack (innermost first)
    /// then the frozen closure scope. Cloning is cheap — every heavy `Value`
    /// variant (Str, Bin, Rec, Tup, Func, …) is already Arc-wrapped.
    fn get(&self, name: &str) -> Option<Value> {
        for frame in self.call_stack.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        self.closure_scope.as_ref().and_then(|s| s.lookup(name))
    }

    /// Get the slot itself (for sharing across scopes — used at function
    /// definition to thread the function's name into its own captured scope).
    fn get_slot(&self, name: &str) -> Option<super::scope::Slot> {
        for frame in self.call_stack.iter().rev() {
            if let Some(s) = frame.get_slot(name) {
                return Some(s.clone());
            }
        }
        self.closure_scope
            .as_ref()
            .and_then(|s| s.lookup_slot(name))
            .cloned()
    }

    /// `let`-style binding: shadows any existing binding by creating a new slot.
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
            let drop_id = self.protocol_methods.drop;
            let _ = self.call_method_by_id(&value, drop_id, &[], out);
            self.dropping = false;
        }
        // Recursively drop struct fields.
        if let Value::Rec { fields, .. } | Value::Tup { fields, .. } = &value {
            for field_val in fields.iter() {
                self.drop_value(field_val.clone(), out);
            }
        }
    }

    /// Update an existing variable in the nearest enclosing scope that
    /// contains it. Walks the live call stack first, then the frozen
    /// closure scope chain — slots are shared, so writing through a
    /// closure scope's slot mutates the original binding too. Returns the
    /// old value (if any) for drop dispatch by the caller.
    fn update_in_scope(&mut self, name: &str, value: Value) -> Result<Option<Value>, RuntimeError> {
        for frame in self.call_stack.iter().rev() {
            if let Some(old) = frame.write(name, value.clone()) {
                return Ok(Some(old));
            }
        }
        if let Some(scope) = self.closure_scope.as_ref() {
            if let Some(slot) = scope.lookup_slot(name) {
                let old = slot.set(value);
                return Ok(Some(old));
            }
        }
        Err(ErrorKind::Undefined {
            kind: NameKind::Variable,
            name: name.to_string(),
        }
        .into())
    }


}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ks::ast::{Expr, Program, Span, Spanned, Stmt};
    use crate::ks::error::FlowMisuse;

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
