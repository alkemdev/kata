use serde::{Deserialize, Serialize};

/// Byte-offset span: `(start, end)` — inclusive start, exclusive end.
pub type Span = (usize, usize);

/// A value of type `T` annotated with its source span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

// ── Statements ────────────────────────────────────────────────────────────────

pub type Program = Vec<Spanned<Stmt>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    /// A bare expression used as a statement.
    Expr(Spanned<Expr>),
    /// `let <name> = <expr>` — variable binding.
    Let { name: String, value: Spanned<Expr> },
    /// `func name(params) { body }` — function definition.
    FuncDef {
        name: String,
        params: Vec<String>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `ret <expr>` — explicit return from the enclosing function.
    Ret(Spanned<Expr>),
}

// ── Expressions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// String literal.
    Str(String),
    /// Numeric literal (parsed as f64).
    Num(f64),
    /// Boolean literal.
    Bool(bool),
    /// The nil / null value.
    Nil,
    /// Variable or type name reference.
    Name(String),
    /// `with` block: scoped bindings + body. Produces the last expression's value.
    /// `with x = 1, y = 2 { body }` or `with { body }`.
    With {
        bindings: Vec<(String, Spanned<Expr>)>,
        body: Vec<Spanned<Stmt>>,
    },
    /// Function call: `callee(args...)`.
    Call {
        /// The function being called — an identifier for now.
        callee: String,
        args: Vec<Spanned<Expr>>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanned_new() {
        let s: Spanned<i32> = Spanned::new(42, (3, 7));
        assert_eq!(s.node, 42);
        assert_eq!(s.span, (3, 7));
    }

    #[test]
    fn expr_serde_roundtrip() {
        let expr = Expr::Call {
            callee: "print".into(),
            args: vec![Spanned::new(Expr::Str("hello".into()), (6, 13))],
        };
        let json = serde_json::to_string(&expr).unwrap();
        let back: Expr = serde_json::from_str(&json).unwrap();
        // Spot-check the round-trip without requiring PartialEq on Expr.
        let json2 = serde_json::to_string(&back).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn program_serde_roundtrip() {
        let program: Program = vec![Spanned::new(
            Stmt::Expr(Spanned::new(Expr::Bool(true), (0, 4))),
            (0, 5),
        )];
        let json = serde_json::to_string_pretty(&program).unwrap();
        let back: Program = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string_pretty(&back).unwrap();
        assert_eq!(json, json2);
    }
}
