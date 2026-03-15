use std::io::Write;

use indexmap::IndexMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use super::ast::{Expr, Program, Spanned, Stmt};

// ── Environment ──────────────────────────────────────────────────────────────

/// Lexically-scoped variable bindings.
///
/// A stack of frames: lookup walks from innermost to outermost.
/// `let` always binds in the current (innermost) frame.
/// `push` / `pop` bracket blocks, function bodies, etc.
///
/// Each frame is an `IndexMap` so iteration follows insertion order.
#[derive(Debug)]
pub struct Scope {
    frames: Vec<IndexMap<String, Value>>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            frames: vec![IndexMap::new()],
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
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
pub fn exec_program(program: &Program, out: &mut impl Write) -> Result<(), String> {
    let mut env = Scope::new();
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
        Stmt::Expr(expr) => {
            let val = eval_expr(expr, env, out)?;
            Ok(Flow::Next(val))
        }
        Stmt::Let { name, value } => {
            let val = eval_expr(value, env, out)?;
            env.set(name.clone(), val);
            Ok(Flow::Next(Value::Nil))
        }
        Stmt::Ret(expr) => {
            let val = eval_expr(expr, env, out)?;
            Ok(Flow::Return(val))
        }
    }
}

/// Execute a block of statements, returning the value of the last expression-statement.
///
/// If the block is empty or ends with a non-expression statement, returns `Nil`.
fn exec_block(
    stmts: &[Spanned<Stmt>],
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Value, String> {
    let mut last_val = Value::Nil;
    for stmt in stmts {
        match exec_stmt(stmt, env, out)? {
            Flow::Next(v) => last_val = v,
            Flow::Return(_) => return Err("ret outside of function".to_string()),
        }
    }
    Ok(last_val)
}

fn eval_expr(expr: &Spanned<Expr>, env: &mut Scope, out: &mut impl Write) -> Result<Value, String> {
    trace!(?expr.node, "eval_expr");
    match &expr.node {
        Expr::Nil => Ok(Value::Nil),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Name(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| format!("undefined variable '{name}'")),

        Expr::With { bindings, body } => {
            env.push();
            for (name, val_expr) in bindings {
                let val = eval_expr(val_expr, env, out)?;
                env.set(name.clone(), val);
            }
            let result = exec_block(body, env, out);
            env.pop();
            result
        }

        Expr::Call { callee, args } => call(callee, args, env, out),
    }
}

fn call(
    name: &str,
    args: &[Spanned<Expr>],
    env: &mut Scope,
    out: &mut impl Write,
) -> Result<Value, String> {
    debug!(name, argc = args.len(), "call");
    match name {
        "print" => {
            let mut parts = Vec::with_capacity(args.len());
            for a in args {
                parts.push(eval_expr(a, env, out)?.to_string());
            }
            writeln!(out, "{}", parts.join(" ")).map_err(|e| e.to_string())?;
            Ok(Value::Nil)
        }
        other => Err(format!("unknown function '{other}'")),
    }
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
        assert!(exec_program(&prog, &mut buf).is_ok());
        assert_eq!(buf, b"hi\n");
    }

    #[test]
    fn eval_nil_literal() {
        // nil as an argument evaluates without error.
        let prog = program(vec![call_stmt("print", vec![s(Expr::Nil)])]);
        assert!(exec_program(&prog, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_bool_literals() {
        let prog = program(vec![call_stmt(
            "print",
            vec![s(Expr::Bool(true)), s(Expr::Bool(false))],
        )]);
        assert!(exec_program(&prog, &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_undefined_variable_is_error() {
        let prog = program(vec![call_stmt("print", vec![s(Expr::Name("x".into()))])]);
        assert!(exec_program(&prog, &mut Vec::new()).is_err());
    }

    #[test]
    fn eval_unknown_function_is_error() {
        let prog = program(vec![call_stmt("undefined_fn", vec![])]);
        let err = exec_program(&prog, &mut Vec::new()).unwrap_err();
        assert!(err.contains("unknown function"), "unexpected error: {err}");
    }

    #[test]
    fn eval_empty_program() {
        assert!(exec_program(&program(vec![]), &mut Vec::new()).is_ok());
    }

    #[test]
    fn eval_ret_outside_function_is_error() {
        let prog = program(vec![s(Stmt::Ret(s(Expr::Num(42.0))))]);
        let err = exec_program(&prog, &mut Vec::new()).unwrap_err();
        assert!(
            err.contains("ret outside of function"),
            "unexpected error: {err}"
        );
    }
}
