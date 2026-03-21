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
    FuncDef(FuncDef),
    /// `enum Name[T] { Variant(T), Unit }` — enum type definition.
    EnumDef {
        name: String,
        type_params: Vec<String>,
        variants: Vec<AstVariantDef>,
    },
    /// `kind Name[T] { field: Type, ... }` — product type definition.
    KindDef {
        name: String,
        type_params: Vec<String>,
        fields: Vec<AstFieldDef>,
    },
    /// `type Name[T] { method_sig* }` — abstract interface definition.
    InterfaceDef {
        name: String,
        type_params: Vec<String>,
        methods: Vec<MethodSig>,
    },
    /// `impl Name[T, ...]? (as Name)? { func_def* }` — attach methods to a kind or enum.
    Impl {
        type_name: String,
        type_params: Vec<String>,
        as_type: Option<Spanned<Expr>>,
        methods: Vec<Spanned<FuncDef>>,
    },
    /// `target = expr` — reassign an existing variable or field.
    Assign {
        target: AssignTarget,
        value: Spanned<Expr>,
    },
    /// `import std.mem` — load a module.
    Import { path: Vec<String> },
    /// `break` — exit the current loop.
    Break,
    /// `continue` — skip to the next loop iteration.
    Continue,
    /// `ret <expr>` — explicit return from the enclosing function.
    Ret(Spanned<Expr>),
}

/// A function parameter with optional type annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    /// Type annotation — a full expression AST (e.g., `Int`, `List[T]`).
    pub type_ann: Option<Spanned<Expr>>,
}

/// A single variant in an enum definition (AST level).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstVariantDef {
    pub name: String,
    /// Field type annotations as full expression ASTs. Empty for unit variants.
    pub fields: Vec<Spanned<Expr>>,
}

/// A field in a type definition. Type annotation is a full expression AST,
/// not a string — supports `x: Int`, `items: List[T]`, `key: Map[Str, V]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstFieldDef {
    pub name: String,
    pub type_ann: Spanned<Expr>,
}

/// A full function definition — name, params, optional return type, body.
/// Used by `Stmt::FuncDef` and `Stmt::Impl.methods`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncDef {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Option<Spanned<Expr>>,
    pub body: Vec<Spanned<Stmt>>,
}

/// A method signature in an interface definition (no body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Option<Spanned<Expr>>,
}

/// Assignment target. Object is a full expression — supports `a.b.c = v`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssignTarget {
    Name(String),
    Attr {
        object: Box<Spanned<Expr>>,
        attr: String,
    },
}

/// A segment of a string interpolation expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterpPart {
    /// Literal text segment.
    Lit(String),
    /// A parsed expression to evaluate and stringify.
    Expr(Spanned<Expr>),
}

// ── Operators ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl BinOp {
    /// The `ops.*` function name for this operator.
    pub fn method_name(self) -> &'static str {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::Eq => "eq",
            BinOp::Ne => "ne",
            BinOp::Lt => "lt",
            BinOp::Gt => "gt",
            BinOp::Le => "le",
            BinOp::Ge => "ge",
        }
    }

    /// Symbolic representation for error messages.
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
}

impl UnaryOp {
    pub fn method_name(self) -> &'static str {
        match self {
            UnaryOp::Neg => "neg",
            UnaryOp::Not => "not",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        }
    }
}

// ── Expressions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// String literal.
    Str(String),
    /// String interpolation: `"hello {name}, {1+2}"` — parts evaluated and concatenated.
    Interp { parts: Vec<InterpPart> },
    /// Integer literal (arbitrary precision, stored as raw string for lossless parsing).
    Int(String),
    /// Float literal (stored as raw string for lossless parsing).
    Float(String),
    /// Boolean literal.
    Bool(bool),
    /// The nil / null value.
    Nil,
    /// Variable or type name reference.
    Name(String),
    /// `with` block: scoped bindings + body. Produces the last expression's value.
    With {
        bindings: Vec<(String, Spanned<Expr>)>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `unsafe { stmts }` — block with unsafe intrinsic access.
    Unsafe { body: Vec<Spanned<Stmt>> },
    /// Attribute access: `a.b`
    Attr {
        object: Box<Spanned<Expr>>,
        name: String,
    },
    /// Item access / type args: `a[b, c]`
    Item {
        object: Box<Spanned<Expr>>,
        args: Vec<Spanned<Expr>>,
    },
    /// Function / constructor call: `a(b, c)`
    Call {
        callee: Box<Spanned<Expr>>,
        args: Vec<Spanned<Expr>>,
    },
    /// Binary operator: `a + b`, `a == b`, etc.
    BinOp {
        op: BinOp,
        left: Box<Spanned<Expr>>,
        right: Box<Spanned<Expr>>,
    },
    /// Unary operator: `-a`, `!a`
    UnaryOp {
        op: UnaryOp,
        operand: Box<Spanned<Expr>>,
    },
    /// `if cond { body } else { body }` — expression-oriented, returns last value.
    If {
        cond: Box<Spanned<Expr>>,
        then_body: Vec<Spanned<Stmt>>,
        else_body: Option<Vec<Spanned<Stmt>>>,
    },
    /// `for x in expr { body }` — iterate via the iterator protocol.
    For {
        binding: String,
        iter_expr: Box<Spanned<Expr>>,
        body: Vec<Spanned<Stmt>>,
    },
    /// `while cond { body }` — loop while condition is truthy, returns nil.
    While {
        cond: Box<Spanned<Expr>>,
        body: Vec<Spanned<Stmt>>,
    },
    /// Short-circuit and: `a && b` — evaluates `b` only if `truth(a)`.
    And {
        left: Box<Spanned<Expr>>,
        right: Box<Spanned<Expr>>,
    },
    /// Short-circuit or: `a || b` — evaluates `b` only if `!truth(a)`.
    Or {
        left: Box<Spanned<Expr>>,
        right: Box<Spanned<Expr>>,
    },
    /// Struct construction: `Point { x: 1.0, y: 2.0 }`
    Construct {
        type_expr: Box<Spanned<Expr>>,
        fields: Vec<(String, Spanned<Expr>)>,
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
            callee: Box::new(Spanned::new(Expr::Name("print".into()), (0, 5))),
            args: vec![Spanned::new(Expr::Str("hello".into()), (6, 13))],
        };
        let json = serde_json::to_string(&expr).unwrap();
        let back: Expr = serde_json::from_str(&json).unwrap();
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
