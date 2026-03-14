use std::io::Write;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use super::ast::{Expr, Program, Spanned, Stmt};

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

/// Execute a fully-parsed program, writing side-effects to `out`.
pub fn exec_program(program: &Program, out: &mut impl Write) -> Result<(), String> {
    debug!(stmts = program.len(), "exec_program");
    for stmt in program {
        exec_stmt(stmt, out)?;
    }
    Ok(())
}

fn exec_stmt(stmt: &Spanned<Stmt>, out: &mut impl Write) -> Result<(), String> {
    trace!(?stmt.node, "exec_stmt");
    match &stmt.node {
        Stmt::Expr(expr) => {
            eval_expr(expr, out)?;
            Ok(())
        }
    }
}

fn eval_expr(expr: &Spanned<Expr>, out: &mut impl Write) -> Result<Value, String> {
    trace!(?expr.node, "eval_expr");
    match &expr.node {
        Expr::Nil => Ok(Value::Nil),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Ident(name) => Err(format!("undefined variable '{name}'")),

        Expr::Call { callee, args } => call(callee, args, out),
    }
}

fn call(name: &str, args: &[Spanned<Expr>], out: &mut impl Write) -> Result<Value, String> {
    debug!(name, argc = args.len(), "call");
    match name {
        "print" => {
            let parts: Vec<String> = args
                .iter()
                .map(|a| eval_expr(a, out).map(|v| v.to_string()))
                .collect::<Result<_, _>>()?;
            writeln!(out, "{}", parts.join(", ")).map_err(|e| e.to_string())?;
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
        let prog = program(vec![call_stmt("print", vec![s(Expr::Ident("x".into()))])]);
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
}
