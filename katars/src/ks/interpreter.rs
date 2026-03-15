use std::io::Write;

use indexmap::IndexMap;
use num_bigint::BigInt;
use tracing::{debug, trace};

use super::ast::{AstVariantDef, Expr, Param, Program, Spanned, Stmt};
use super::types::{prim, ResolvedVariantDef, TypeDef, TypeExpr, TypeId, TypeRegistry, VariantDef};
use super::value::{FuncParam, Value};

// ── Flow ─────────────────────────────────────────────────────────────────────

/// Outcome of executing a statement or block.
#[derive(Debug)]
pub enum Flow {
    /// Statement completed normally. Carries the value for expression-statements.
    Next(Value),
    /// A `ret` statement was hit; carry the value up to the call site.
    Return(Value),
}

// ── Interpreter ──────────────────────────────────────────────────────────────

/// The KataScript interpreter. Owns the type registry, variable scopes,
/// and all evaluation logic.
pub struct Interpreter {
    /// All registered types.
    pub types: TypeRegistry,
    /// Lexically-scoped variable frames. Lookup walks innermost to outermost.
    frames: Vec<IndexMap<String, Value>>,
}

impl Interpreter {
    /// Create a new interpreter with primitive types bootstrapped.
    pub fn new() -> Self {
        let types = TypeRegistry::new();
        let mut interp = Self {
            types,
            frames: vec![IndexMap::new()],
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

    /// Execute a program, optionally loading a prelude first.
    pub fn exec_program(
        &mut self,
        program: &Program,
        prelude: Option<&Program>,
        out: &mut impl Write,
    ) -> Result<(), String> {
        if let Some(pre) = prelude {
            debug!(stmts = pre.len(), "loading prelude");
            for stmt in pre {
                match self.exec_stmt(stmt, out)? {
                    Flow::Next(_) => {}
                    Flow::Return(_) => return Err("ret in prelude".to_string()),
                }
            }
        }

        debug!(stmts = program.len(), "exec_program");
        for stmt in program {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(_) => {}
                Flow::Return(_) => return Err("ret outside of function".to_string()),
            }
        }
        Ok(())
    }

    // ── Statement execution ──────────────────────────────────────────────

    fn exec_stmt(&mut self, stmt: &Spanned<Stmt>, out: &mut impl Write) -> Result<Flow, String> {
        trace!(?stmt.node, "exec_stmt");
        match &stmt.node {
            Stmt::Expr(expr) => self.eval_expr(expr, out),

            Stmt::EnumDef {
                name,
                type_params,
                variants,
            } => {
                self.register_enum(name, type_params, variants)?;
                Ok(Flow::Next(Value::Nil))
            }

            Stmt::FuncDef {
                name,
                params,
                ret_type,
                body,
            } => {
                let func_params: Vec<FuncParam> = params
                    .iter()
                    .map(|p| {
                        let type_id = p
                            .type_name
                            .as_ref()
                            .map(|tn| self.resolve_type(tn))
                            .transpose()?;
                        Ok(FuncParam {
                            name: p.name.clone(),
                            type_id,
                        })
                    })
                    .collect::<Result<_, String>>()?;

                let ret_tid = ret_type
                    .as_ref()
                    .map(|rt| self.resolve_type(rt))
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
    ) -> Result<Flow, String> {
        let mut last_val = Value::Nil;
        for stmt in stmts {
            match self.exec_stmt(stmt, out)? {
                Flow::Next(v) => last_val = v,
                ret @ Flow::Return(_) => return Ok(ret),
            }
        }
        Ok(Flow::Next(last_val))
    }

    // ── Expression evaluation ────────────────────────────────────────────

    fn eval_expr(&mut self, expr: &Spanned<Expr>, out: &mut impl Write) -> Result<Flow, String> {
        trace!(?expr.node, "eval_expr");
        match &expr.node {
            Expr::Nil => Ok(Flow::Next(Value::Nil)),
            Expr::Bool(b) => Ok(Flow::Next(Value::Bool(*b))),
            Expr::Int(s) => {
                let n: BigInt = s
                    .parse()
                    .map_err(|e| format!("invalid integer literal '{s}': {e}"))?;
                Ok(Flow::Next(Value::Int(n)))
            }
            Expr::Float(s) => {
                let n: f64 = s
                    .parse()
                    .map_err(|e| format!("invalid float literal '{s}': {e}"))?;
                Ok(Flow::Next(Value::Float(n)))
            }
            Expr::Str(s) => Ok(Flow::Next(Value::Str(s.clone()))),

            Expr::Name(name) => self
                .get(name)
                .cloned()
                .map(Flow::Next)
                .ok_or_else(|| format!("undefined variable '{name}'")),

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
            }

            Expr::Item { object, args } => {
                let obj = self.eval_value(object, out)?;
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
                }
                self.eval_item(&obj, &arg_vals)
            }

            Expr::Call { callee, args } => {
                // Evaluate args once.
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_value(a, out)?);
                }

                // Fast path: if callee is a bare Name, check builtins first.
                if let Expr::Name(name) = &callee.node {
                    if let Some(result) = self.call_builtin(name, &arg_vals, out)? {
                        return Ok(result);
                    }
                }

                let func = self.eval_value(callee, out)?;
                self.eval_call(func, &arg_vals, out)
            }
        }
    }

    /// Evaluate an expression and extract the value (not a return flow).
    fn eval_value(&mut self, expr: &Spanned<Expr>, out: &mut impl Write) -> Result<Value, String> {
        match self.eval_expr(expr, out)? {
            Flow::Next(v) | Flow::Return(v) => Ok(v),
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
                .map(|f| {
                    // If the field name matches a type param, it's a Param reference.
                    // Otherwise, try to resolve it as a concrete type.
                    if type_params.contains(f) {
                        TypeExpr::Param(f.clone())
                    } else {
                        match self.resolve_type(f) {
                            Ok(tid) => TypeExpr::Concrete(tid),
                            Err(_) => TypeExpr::Param(f.clone()), // treat as param
                        }
                    }
                })
                .collect();
            variants.insert(v.name.clone(), VariantDef { fields });
        }

        let type_id = self
            .types
            .register_enum(name.to_string(), type_params.to_vec(), variants);

        // Put the type in scope as a Value::Type.
        self.set(name.to_string(), Value::Type(type_id));
        Ok(())
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
                    _ => Err(format!(
                        "cannot access '.{name}' on type '{}'",
                        self.types.display_name(*type_id)
                    )),
                }
            }
            other => Err(format!(
                "cannot access '.{name}' on {}",
                self.types.display_name(other.type_id())
            )),
        }
    }

    // ── Item: a[b] ───────────────────────────────────────────────────────

    fn eval_item(&mut self, object: &Value, args: &[Value]) -> Result<Flow, String> {
        match object {
            // Type[Args] — generic enum instantiation
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
                let instance_id = self.types.instantiate_enum(*base_id, type_args)?;
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

                // Type-check arguments.
                for (param, val) in params.iter().zip(args.iter()) {
                    if let Some(expected) = param.type_id {
                        self.check_type(val, expected)?;
                    }
                }

                // Push scope, bind params.
                self.push_scope();
                for (param, val) in params.iter().zip(args.iter()) {
                    self.set(param.name.clone(), val.clone());
                }

                let result = match self.exec_block(&body, out)? {
                    Flow::Next(v) => v,
                    Flow::Return(v) => v,
                };

                self.pop_scope();

                // Type-check return value.
                if let Some(expected_ret) = ret_type {
                    self.check_type(&result, expected_ret)?;
                }

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

            other => Err(format!(
                "'{}' is not callable",
                self.types.display_name(other.type_id())
            )),
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
            err.contains("undefined variable"),
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
            err.contains("ret outside of function"),
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
