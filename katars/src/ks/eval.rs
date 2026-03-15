use std::io::Write;

use indexmap::IndexMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use super::ast::{Expr, Program, Spanned, Stmt, VariantDef};

// ── Enum type registry ───────────────────────────────────────────────────────

/// A registered enum type definition.
#[derive(Debug, Clone)]
pub struct EnumType {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: IndexMap<String, VariantInfo>,
}

/// Info about a single variant: how many fields it expects.
#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub field_count: usize,
}

// ── Environment ──────────────────────────────────────────────────────────────

/// Lexically-scoped variable bindings and type definitions.
///
/// A stack of frames: lookup walks from innermost to outermost.
/// `let` always binds in the current (innermost) frame.
/// `push` / `pop` bracket blocks, function bodies, etc.
///
/// Each frame is an `IndexMap` so iteration follows insertion order.
#[derive(Debug)]
pub struct Scope {
    frames: Vec<IndexMap<String, Value>>,
    /// Enum type definitions, keyed by name. Not scoped — enums are global.
    types: IndexMap<String, EnumType>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            frames: vec![IndexMap::new()],
            types: IndexMap::new(),
        }
    }

    /// Look up a name, walking from innermost frame outward.
    pub fn get(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        None
    }

    /// Bind a name in the current (innermost) frame.
    pub fn set(&mut self, name: String, value: Value) {
        self.frames
            .last_mut()
            .expect("scope always has at least one frame")
            .insert(name, value);
    }

    /// Register an enum type definition.
    pub fn define_enum(&mut self, def: EnumType) {
        self.types.insert(def.name.clone(), def);
    }

    /// Look up an enum type definition.
    pub fn get_enum(&self, name: &str) -> Option<&EnumType> {
        self.types.get(name)
    }

    /// Enter a new inner frame.
    pub fn push(&mut self) {
        self.frames.push(IndexMap::new());
    }

    /// Leave the current inner frame, discarding its bindings.
    pub fn pop(&mut self) {
        debug_assert!(self.frames.len() > 1, "cannot pop the global frame");
        self.frames.pop();
    }
}

// ── Runtime value ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    /// A user-defined function: parameter names + body AST.
    Func {
        params: Vec<String>,
        body: Vec<Spanned<Stmt>>,
    },
    /// An enum variant value.
    Enum {
        type_name: String,
        variant: String,
        fields: Vec<Value>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Func { .. }, Value::Func { .. }) => false,
            (
                Value::Enum {
                    type_name: t1,
                    variant: v1,
                    fields: f1,
                },
                Value::Enum {
                    type_name: t2,
                    variant: v2,
                    fields: f2,
                },
            ) => t1 == t2 && v1 == v2 && f1 == f2,
            _ => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Nil => write!(f, "nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Value::Str(s) => write!(f, "{s}"),
            Value::Func { params, .. } => write!(f, "<func({})>", params.join(", ")),
            Value::Enum {
                variant, fields, ..
            } => {
                if fields.is_empty() {
                    write!(f, "{variant}")
                } else {
                    let inner: Vec<String> = fields.iter().map(|v| v.to_string()).collect();
                    write!(f, "{variant}({})", inner.join(", "))
                }
            }
        }
    }
}

// ── Interpreter ───────────────────────────────────────────────────────────────

/// Outcome of executing a statement or block.
///
/// This lets `ret` unwind cleanly without being treated as an error.
/// The enclosing `call` (once user functions are implemented) catches
/// `Flow::Return(v)` and uses it as the function's result value.
#[derive(Debug)]
pub enum Flow {
    /// Statement completed normally. Carries the value for expression-statements,
    /// `Nil` for non-expression statements (let, etc.).
    Next(Value),
    /// A `ret` statement was hit; carry the value up to the call site.
    Return(Value),
}

/// Execute a fully-parsed program, writing side-effects to `out`.
///
/// If `prelude` is provided, it is executed first to populate the environment
/// with standard definitions (Opt, Res, etc.).
pub fn exec_program(
    program: &Program,
    prelude: Option<&Program>,
    out: &mut impl Write,
) -> Result<(), String> {
    let mut env = Scope::new();

    // Load prelude into the environment.
    if let Some(pre) = prelude {
        debug!(stmts = pre.len(), "loading prelude");
        for stmt in pre {
            match exec_stmt(stmt, &mut env, out)? {
                Flow::Next(_) => {}
                Flow::Return(_) => return Err("ret in prelude".to_string()),
            }
        }
    }

    debug!(stmts = program.len(), "exec_program");
    for stmt in program {
        match exec_stmt(stmt, &mut env, out)? {
            Flow::Next(_) => {}
            Flow::Return(_) => return Err("ret outside of function".to_string()),
        }
    }
    Ok(())
}

/// Execute one statement and return the control-flow outcome.
pub fn exec_stmt(
    stmt: &Spanned<Stmt>,
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Flow, String> {
    trace!(?stmt.node, "exec_stmt");
    match &stmt.node {
        Stmt::Expr(expr) => eval_expr(expr, env, out),
        Stmt::EnumDef {
            name,
            type_params,
            variants,
        } => {
            let mut variant_map = IndexMap::new();
            for v in variants {
                variant_map.insert(
                    v.name.clone(),
                    VariantInfo {
                        field_count: v.fields.len(),
                    },
                );
            }
            env.define_enum(EnumType {
                name: name.clone(),
                type_params: type_params.clone(),
                variants: variant_map,
            });
            Ok(Flow::Next(Value::Nil))
        }
        Stmt::FuncDef { name, params, body } => {
            let func = Value::Func {
                params: params.clone(),
                body: body.clone(),
            };
            env.set(name.clone(), func);
            Ok(Flow::Next(Value::Nil))
        }
        Stmt::Let { name, value } => {
            let val = eval_value(value, env, out)?;
            env.set(name.clone(), val);
            Ok(Flow::Next(Value::Nil))
        }
        Stmt::Ret(expr) => {
            let val = eval_value(expr, env, out)?;
            Ok(Flow::Return(val))
        }
    }
}

/// Execute a block of statements, returning the control flow outcome.
///
/// Returns `Flow::Next(v)` where `v` is the last expression-statement's value,
/// or `Flow::Return(v)` if a `ret` was hit (propagated to the caller).
fn exec_block(
    stmts: &[Spanned<Stmt>],
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Flow, String> {
    let mut last_val = Value::Nil;
    for stmt in stmts {
        match exec_stmt(stmt, env, out)? {
            Flow::Next(v) => last_val = v,
            ret @ Flow::Return(_) => return Ok(ret),
        }
    }
    Ok(Flow::Next(last_val))
}

/// Evaluate an expression, returning `Flow` to support `ret` propagation
/// through `with` blocks.
fn eval_expr(expr: &Spanned<Expr>, env: &mut Scope, out: &mut impl Write) -> Result<Flow, String> {
    trace!(?expr.node, "eval_expr");
    match &expr.node {
        Expr::Nil => Ok(Flow::Next(Value::Nil)),
        Expr::Bool(b) => Ok(Flow::Next(Value::Bool(*b))),
        Expr::Num(n) => Ok(Flow::Next(Value::Num(*n))),
        Expr::Str(s) => Ok(Flow::Next(Value::Str(s.clone()))),
        Expr::Name(name) => env
            .get(name)
            .cloned()
            .map(Flow::Next)
            .ok_or_else(|| format!("undefined variable '{name}'")),

        Expr::With { bindings, body } => {
            env.push();
            for (name, val_expr) in bindings {
                let val = eval_value(val_expr, env, out)?;
                env.set(name.clone(), val);
            }
            let result = exec_block(body, env, out);
            env.pop();
            result
        }

        Expr::EnumVariant {
            enum_name,
            type_args: _,
            variant,
            args,
        } => {
            let enum_def = env
                .get_enum(enum_name)
                .ok_or_else(|| format!("undefined type '{enum_name}'"))?
                .clone();

            let variant_info = enum_def
                .variants
                .get(variant.as_str())
                .ok_or_else(|| format!("'{enum_name}' has no variant '{variant}'"))?
                .clone();

            if args.len() != variant_info.field_count {
                return Err(format!(
                    "'{variant}' expects {} argument(s), got {}",
                    variant_info.field_count,
                    args.len()
                ));
            }

            let mut field_vals = Vec::with_capacity(args.len());
            for a in args {
                field_vals.push(eval_value(a, env, out)?);
            }

            Ok(Flow::Next(Value::Enum {
                type_name: enum_name.clone(),
                variant: variant.clone(),
                fields: field_vals,
            }))
        }

        Expr::Call { callee, args } => call(callee, args, env, out),
    }
}

/// Convenience: evaluate an expression and expect a value (not a return).
/// Used in contexts where `ret` propagation is handled by the caller.
fn eval_value(
    expr: &Spanned<Expr>,
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Value, String> {
    match eval_expr(expr, env, out)? {
        Flow::Next(v) => Ok(v),
        Flow::Return(v) => Ok(v), // In `let x = with { ret 1 }`, the ret value becomes x
    }
}

fn call(
    name: &str,
    args: &[Spanned<Expr>],
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Flow, String> {
    debug!(name, argc = args.len(), "call");

    // Built-in functions.
    match name {
        "print" => {
            let mut parts = Vec::with_capacity(args.len());
            for a in args {
                parts.push(eval_value(a, env, out)?.to_string());
            }
            writeln!(out, "{}", parts.join(" ")).map_err(|e| e.to_string())?;
            return Ok(Flow::Next(Value::Nil));
        }
        _ => {}
    }

    // User-defined functions.
    let func = env
        .get(name)
        .cloned()
        .ok_or_else(|| format!("unknown function '{name}'"))?;

    let Value::Func { params, body } = func else {
        return Err(format!("'{name}' is not a function"));
    };

    // Evaluate arguments before pushing the new scope.
    let mut arg_vals = Vec::with_capacity(args.len());
    for a in args {
        arg_vals.push(eval_value(a, env, out)?);
    }

    if arg_vals.len() != params.len() {
        return Err(format!(
            "'{name}' expects {} argument(s), got {}",
            params.len(),
            arg_vals.len()
        ));
    }

    // Push function scope, bind params.
    env.push();
    for (param, val) in params.iter().zip(arg_vals) {
        env.set(param.clone(), val);
    }

    // Execute body — catch Return as the function's result.
    let result = match exec_block(&body, env, out)? {
        Flow::Next(v) => v,
        Flow::Return(v) => v,
    };

    env.pop();
    Ok(Flow::Next(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ks::ast::{Expr, Stmt};

    /// Build a zero-span Spanned<T> — good enough for eval tests.
    fn s<T>(node: T) -> Spanned<T> {
        Spanned::new(node, (0, 0))
    }

    fn program(stmts: Vec<Spanned<Stmt>>) -> Program {
        stmts
    }

    fn call_stmt(callee: &str, args: Vec<Spanned<Expr>>) -> Spanned<Stmt> {
        s(Stmt::Expr(s(Expr::Call {
            callee: callee.to_string(),
            args,
        })))
    }

    // ── Value::Display ────────────────────────────────────────────────────────

    #[test]
    fn display_nil() {
        assert_eq!(Value::Nil.to_string(), "nil");
    }

    #[test]
    fn display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn display_integer_num() {
        // Whole floats should display without decimal point.
        assert_eq!(Value::Num(42.0).to_string(), "42");
        assert_eq!(Value::Num(-1.0).to_string(), "-1");
        assert_eq!(Value::Num(0.0).to_string(), "0");
    }

    #[test]
    fn display_fractional_num() {
        assert_eq!(Value::Num(3.14).to_string(), "3.14");
    }

    #[test]
    fn display_str() {
        assert_eq!(Value::Str("hello".into()).to_string(), "hello");
    }

    // ── Value serde ───────────────────────────────────────────────────────────

    #[test]
    fn value_serde_roundtrip() {
        for v in [
            Value::Nil,
            Value::Bool(true),
            Value::Num(1.5),
            Value::Str("x".into()),
        ] {
            let json = serde_json::to_string(&v).unwrap();
            let back: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    // ── eval_expr ─────────────────────────────────────────────────────────────

    #[test]
    fn eval_literals() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Str("hi".into()))])]);
        let mut buf = Vec::new();
        assert!(exec_program(&prog, None, &mut buf).is_ok());
        assert_eq!(buf, b"hi\n");
    }

    #[test]
    fn eval_nil_literal() {
        // nil as an argument evaluates without error.
        let prog = program(vec![call_stmt("print", vec![s(Expr::Nil)])]);
        assert!(exec_program(&prog, None, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_bool_literals() {
        let prog = program(vec![call_stmt(
            "print",
            vec![s(Expr::Bool(true)), s(Expr::Bool(false))],
        )]);
        assert!(exec_program(&prog, None, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_undefined_variable_is_error() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Name("x".into()))])]);
        assert!(exec_program(&prog, None, &mut Vec::new()).is_err());
    }

    #[test]
    fn eval_unknown_function_is_error() {
        let prog = program(vec![call_stmt("undefined_fn", vec![])]);
        let err = exec_program(&prog, None, &mut Vec::new()).unwrap_err();
        assert!(err.contains("unknown function"), "unexpected error: {err}");
    }

    #[test]
    fn eval_empty_program() {
        assert!(exec_program(&program(vec![]), None, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_ret_outside_function_is_error() {
        let prog = program(vec![s(Stmt::Ret(s(Expr::Num(42.0))))]);
        let err = exec_program(&prog, None, &mut Vec::new()).unwrap_err();
        assert!(
            err.contains("ret outside of function"),
            "unexpected error: {err}"
        );
    }
}
