use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::{
    input::{Stream, ValueInput},
    pratt::*,
    prelude::*,
};
use logos::Logos;
use tracing::{debug, info};

use super::ast::{AstVariantDef, BinOp, Expr, Param, Program, Spanned, Stmt, UnaryOp};
use super::lexer::Token;

// ── Grammar ───────────────────────────────────────────────────────────────────
//
//   program    = stmt*
//   stmt       = 'enum' IDENT type_params? '{' variant_list '}'      -- enum def
//              | 'func' IDENT '(' params? ')' ret_ann? '{' stmt* '}' -- function def
//              | 'let' IDENT '=' expr ';'?                            -- variable binding
//              | IDENT '=' expr ';'?                                    -- assignment
//              | 'ret' expr ';'?                                       -- explicit return
//              | expr ';'?
//   type_params = '[' IDENT (',' IDENT)* ']'
//   variant_list = variant (',' variant)* ','?
//   variant    = IDENT '(' IDENT (',' IDENT)* ')'  -- data variant
//              | IDENT                               -- unit variant
//   params     = param (',' param)*
//   param      = IDENT ':' IDENT                    -- typed param
//              | IDENT                               -- untyped param
//   ret_ann    = ':' IDENT                           -- return type annotation
//   expr       = with_expr | if_expr | while_expr | op_expr
//   while_expr = 'while' expr '{' stmt* '}'
//   with_expr  = 'with' (binding (',' binding)*)? '{' stmt* '}'
//   binding    = IDENT '=' expr
//   if_expr    = 'if' expr '{' stmt* '}' ('else' '{' stmt* '}' | 'elif' if_expr)?
//   op_expr    = unary (binop unary)*          -- pratt precedence climbing
//   binop      = '+' | '-' | '*' | '/' | '==' | '!=' | '<' | '>' | '<=' | '>='
//              | '&&' | '||'
//   unary      = ('-' | '!') unary | postfix
//   postfix    = atom ('.' IDENT | '[' args ']' | '(' args ')')*
//   atom       = ident | str | num | 'true' | 'false' | 'nil' | '(' expr ')'

fn span(s: &SimpleSpan) -> (usize, usize) {
    (s.start, s.end)
}

// ── Statement & expression parsers ───────────────────────────────────────────
//
// `with` bodies contain statements, and statements contain expressions, so the
// two are mutually recursive. We use `recursive` at the statement level and
// build the expression parser inline.

fn stmt_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Stmt>, extra::Err<Rich<'tokens, Token>>>
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|stmt| {
        // ── expression parser (uses `stmt` for `with` bodies) ────────────

        let expr = recursive(|expr| {
            // ── atoms ────────────────────────────────────────────────

            let str_lit = select! { Token::Str(s) => Expr::Str(s) };
            let num_lit = select! { Token::Num(n) => {
                if n.contains('.') {
                    Expr::Float(n)
                } else {
                    Expr::Int(n)
                }
            }};
            let bool_lit = select! {
                Token::True  => Expr::Bool(true),
                Token::False => Expr::Bool(false),
            };
            let nil_lit = just(Token::Nil).to(Expr::Nil);
            let name = select! { Token::Ident(s) => Expr::Name(s) };

            let paren = expr
                .clone()
                .delimited_by(just(Token::LParen), just(Token::RParen));

            // with_expr = 'with' (binding (',' binding)*)? '{' stmt* '}'
            let binding = select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Eq))
                .then(expr.clone());

            let with_expr = just(Token::With)
                .ignore_then(
                    binding
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .or_not(),
                )
                .then(
                    stmt.clone()
                        .repeated()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                )
                .map_with(|(bindings, body), ex| {
                    Spanned::new(
                        Expr::With {
                            bindings: bindings.unwrap_or_default(),
                            body,
                        },
                        span(&ex.span()),
                    )
                });

            // if_expr = 'if' expr '{' stmt* '}' ('else' (if_expr | '{' stmt* '}'))?
            let block = stmt
                .clone()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace));

            // if_expr  = 'if' expr block (else_tail)?
            // else_tail = 'else' block
            //           | 'elif' expr block (else_tail)?
            //
            // Only else_tail is recursive (chains elif). The top-level
            // if_expr is not recursive, so consecutive if-statements
            // stay separate.
            let else_tail = recursive(|else_tail| {
                // else { ... }
                just(Token::Else).ignore_then(block.clone()).or(
                    // elif cond { ... } (else_tail)? → nested If as else body
                    just(Token::Elif)
                        .ignore_then(expr.clone())
                        .then(block.clone())
                        .then(else_tail.or_not())
                        .map_with(|((cond, then_body), else_body), ex| {
                            let if_node = Spanned::new(
                                Expr::If {
                                    cond: Box::new(cond),
                                    then_body,
                                    else_body,
                                },
                                span(&ex.span()),
                            );
                            vec![Spanned::new(Stmt::Expr(if_node.clone()), if_node.span)]
                        }),
                )
            });

            let if_expr = just(Token::If)
                .ignore_then(expr.clone())
                .then(block.clone())
                .then(else_tail.or_not())
                .map_with(|((cond, then_body), else_body), ex| {
                    Spanned::new(
                        Expr::If {
                            cond: Box::new(cond),
                            then_body,
                            else_body,
                        },
                        span(&ex.span()),
                    )
                });

            let while_expr = just(Token::While)
                .ignore_then(expr.clone())
                .then(block.clone())
                .map_with(|(cond, body), ex| {
                    Spanned::new(
                        Expr::While {
                            cond: Box::new(cond),
                            body,
                        },
                        span(&ex.span()),
                    )
                });

            let atom = str_lit
                .or(num_lit)
                .or(bool_lit)
                .or(nil_lit)
                .or(name)
                .map_with(|e, ex| Spanned::new(e, span(&ex.span())))
                .or(paren)
                .or(with_expr)
                .or(if_expr)
                .or(while_expr);

            // ── postfix chain: .attr, [item], (call) ─────────────────

            enum Postfix {
                Attr(String),
                Item(Vec<Spanned<Expr>>),
                Call(Vec<Spanned<Expr>>),
            }

            let attr = just(Token::Dot)
                .ignore_then(select! { Token::Ident(name) => name })
                .map(Postfix::Attr);

            let item = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map(Postfix::Item);

            let call = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .map(Postfix::Call);

            let postfix = attr.or(item).or(call);

            let postfix_chain = atom.foldl(postfix.repeated(), |lhs, op| {
                let s = lhs.span;
                match op {
                    Postfix::Attr(name) => Spanned::new(
                        Expr::Attr {
                            object: Box::new(lhs),
                            name,
                        },
                        s,
                    ),
                    Postfix::Item(args) => Spanned::new(
                        Expr::Item {
                            object: Box::new(lhs),
                            args,
                        },
                        s,
                    ),
                    Postfix::Call(args) => Spanned::new(
                        Expr::Call {
                            callee: Box::new(lhs),
                            args,
                        },
                        s,
                    ),
                }
            });

            // ── operator precedence (pratt) ────────────────────────────
            //
            // Precedence (low → high):
            //   0: ||
            //   1: &&
            //   2: == !=
            //   3: < > <= >=
            //   4: + -
            //   5: * /
            //   6: unary - !

            macro_rules! bin {
                ($op:expr) => {
                    |l: Spanned<Expr>, _, r: Spanned<Expr>, _: &mut _| {
                        let s = (l.span.0, r.span.1);
                        Spanned::new(
                            Expr::BinOp {
                                op: $op,
                                left: Box::new(l),
                                right: Box::new(r),
                            },
                            s,
                        )
                    }
                };
            }

            macro_rules! un {
                ($op:expr) => {
                    |_: Token, operand: Spanned<Expr>, _: &mut _| {
                        let s = operand.span;
                        Spanned::new(
                            Expr::UnaryOp {
                                op: $op,
                                operand: Box::new(operand),
                            },
                            s,
                        )
                    }
                };
            }

            postfix_chain.pratt((
                // Unary (highest precedence)
                prefix(6, just(Token::Minus), un!(UnaryOp::Neg)),
                prefix(6, just(Token::Bang), un!(UnaryOp::Not)),
                // Multiplicative
                infix(left(5), just(Token::Star), bin!(BinOp::Mul)),
                infix(left(5), just(Token::Slash), bin!(BinOp::Div)),
                // Additive
                infix(left(4), just(Token::Plus), bin!(BinOp::Add)),
                infix(left(4), just(Token::Minus), bin!(BinOp::Sub)),
                // Comparison
                infix(left(3), just(Token::Lt), bin!(BinOp::Lt)),
                infix(left(3), just(Token::Gt), bin!(BinOp::Gt)),
                infix(left(3), just(Token::LtEq), bin!(BinOp::Le)),
                infix(left(3), just(Token::GtEq), bin!(BinOp::Ge)),
                // Equality
                infix(left(2), just(Token::EqEq), bin!(BinOp::Eq)),
                infix(left(2), just(Token::BangEq), bin!(BinOp::Ne)),
                // Short-circuit logical (own AST nodes, not BinOp)
                infix(
                    left(1),
                    just(Token::And),
                    |l: Spanned<Expr>, _, r: Spanned<Expr>, _: &mut _| {
                        let s = (l.span.0, r.span.1);
                        Spanned::new(
                            Expr::And {
                                left: Box::new(l),
                                right: Box::new(r),
                            },
                            s,
                        )
                    },
                ),
                infix(
                    left(0),
                    just(Token::Or),
                    |l: Spanned<Expr>, _, r: Spanned<Expr>, _: &mut _| {
                        let s = (l.span.0, r.span.1);
                        Spanned::new(
                            Expr::Or {
                                left: Box::new(l),
                                right: Box::new(r),
                            },
                            s,
                        )
                    },
                ),
            ))
        });

        // ── statement parser ─────────────────────────────────────────────

        // variant = IDENT '(' IDENT (',' IDENT)* ')' | IDENT
        let variant_def = select! { Token::Ident(name) => name }
            .then(
                select! { Token::Ident(name) => name }
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .or_not(),
            )
            .map(|(name, fields)| AstVariantDef {
                name,
                fields: fields.unwrap_or_default(),
            });

        // enum_def = 'enum' IDENT type_params? '{' variant_list '}'
        let enum_type_params = select! { Token::Ident(name) => name }
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket));

        let enum_def = just(Token::Enum)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(enum_type_params.or_not())
            .then(
                variant_def
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|((name, type_params), variants), ex| {
                Spanned::new(
                    Stmt::EnumDef {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        variants,
                    },
                    span(&ex.span()),
                )
            });

        // param = IDENT ':' IDENT | IDENT
        let param = select! { Token::Ident(name) => name }
            .then(
                just(Token::Colon)
                    .ignore_then(select! { Token::Ident(name) => name })
                    .or_not(),
            )
            .map(|(name, type_name)| Param { name, type_name });

        // ret_ann = ':' IDENT
        let ret_ann = just(Token::Colon).ignore_then(select! { Token::Ident(name) => name });

        let func_def = just(Token::Func)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(
                param
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(ret_ann.or_not())
            .then(
                stmt.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(((name, params), ret_type), body), ex| {
                Spanned::new(
                    Stmt::FuncDef {
                        name,
                        params,
                        ret_type,
                        body,
                    },
                    span(&ex.span()),
                )
            });

        let let_stmt = just(Token::Let)
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|(name, value), ex| {
                Spanned::new(Stmt::Let { name, value }, span(&ex.span()))
            });

        let ret_stmt = just(Token::Ret)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|expr, ex| Spanned::new(Stmt::Ret(expr), span(&ex.span())));

        let assign_stmt = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|(name, value), ex| {
                Spanned::new(Stmt::Assign { name, value }, span(&ex.span()))
            });

        let expr_stmt = expr
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|expr, ex| Spanned::new(Stmt::Expr(expr), span(&ex.span())));

        enum_def
            .or(func_def)
            .or(let_stmt)
            .or(assign_stmt)
            .or(ret_stmt)
            .or(expr_stmt)
    })
}

fn program_parser<'tokens, I>() -> impl Parser<'tokens, I, Program, extra::Err<Rich<'tokens, Token>>>
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    stmt_parser()
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(end())
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Parse `source` into a `Program` AST.
///
/// Lex errors and parse errors are printed to stderr via ariadne.
/// Returns `Err(())` if any errors occurred.
pub fn parse(source: &str, filename: &str) -> Result<Program, ()> {
    info!(filename, bytes = source.len(), "parsing");

    // Lex: logos iterator → (Token, SimpleSpan) stream.
    let token_iter =
        Token::lexer(source)
            .spanned()
            .map(|(result, span): (_, std::ops::Range<usize>)| {
                let tok = result.unwrap_or(Token::Error);
                (tok, SimpleSpan::from(span))
            });

    let token_stream = Stream::from_iter(token_iter).map(
        SimpleSpan::from(source.len()..source.len()),
        |(t, s): (_, _)| (t, s),
    );

    // Parse.
    let (ast, errors) = program_parser().parse(token_stream).into_output_errors();

    debug!(
        stmts = ast.as_ref().map(|p| p.len()).unwrap_or(0),
        errors = errors.len(),
        "parse done"
    );

    let had_errors = !errors.is_empty();

    for err in errors {
        let span = err.span().into_range();

        Report::build(ReportKind::Error, filename, span.start)
            .with_message(err.to_string())
            .with_label(
                Label::new((filename, span))
                    .with_message(err.reason().to_string())
                    .with_color(Color::Red),
            )
            .with_labels(err.contexts().map(|(label, span)| {
                Label::new((filename, span.into_range()))
                    .with_message(format!("while parsing {label}"))
                    .with_color(Color::Yellow)
            }))
            .finish()
            .eprint((filename, Source::from(source)))
            .unwrap();
    }

    if had_errors {
        Err(())
    } else {
        ast.ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ks::ast::Expr;

    fn parse_ok(src: &str) -> Program {
        parse(src, "<test>").expect("expected successful parse")
    }

    fn parse_err(src: &str) {
        parse(src, "<test>").expect_err("expected parse failure");
    }

    // ── happy path ────────────────────────────────────────────────────────────

    #[test]
    fn parse_empty_program() {
        assert_eq!(parse_ok("").len(), 0);
    }

    #[test]
    fn parse_print_call() {
        let prog = parse_ok(r#"print("hello, world");"#);
        assert_eq!(prog.len(), 1);
        let Stmt::Expr(ref expr) = prog[0].node else {
            panic!("expected Expr stmt, got {:?}", prog[0].node)
        };
        let Expr::Call {
            ref callee,
            ref args,
        } = expr.node
        else {
            panic!("expected Call, got {:?}", expr.node);
        };
        assert!(matches!(callee.node, Expr::Name(ref n) if n == "print"));
        assert_eq!(args.len(), 1);
        assert!(matches!(args[0].node, Expr::Str(ref s) if s == "hello, world"));
    }

    #[test]
    fn parse_multiple_statements() {
        // With semicolons.
        let prog = parse_ok(r#"print("a"); print("b");"#);
        assert_eq!(prog.len(), 2);
        // Without semicolons.
        let prog = parse_ok("print(true)\nprint(false)");
        assert_eq!(prog.len(), 2);
    }

    #[test]
    fn parse_bool_literals() {
        let prog = parse_ok("print(true); print(false);");
        let Stmt::Expr(ref a) = prog[0].node else {
            panic!()
        };
        let Expr::Call { ref args, .. } = a.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Bool(true)));

        let Stmt::Expr(ref b) = prog[1].node else {
            panic!()
        };
        let Expr::Call { ref args, .. } = b.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Bool(false)));
    }

    #[test]
    fn parse_nil_literal() {
        let prog = parse_ok("print(nil);");
        let Stmt::Expr(ref expr) = prog[0].node else {
            panic!()
        };
        let Expr::Call { ref args, .. } = expr.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Nil));
    }

    #[test]
    fn parse_number_literal() {
        let prog = parse_ok("print(42);");
        let Stmt::Expr(ref expr) = prog[0].node else {
            panic!()
        };
        let Expr::Call { ref args, .. } = expr.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Int(ref s) if s == "42"));
    }

    #[test]
    fn parse_span_covers_statement() {
        let prog = parse_ok(r#"print("hi");"#);
        // Statement span should start at 0 and end at or past the semicolon.
        let (start, end) = prog[0].span;
        assert_eq!(start, 0);
        assert!(end > 0);
    }

    #[test]
    fn parse_ret_stmt() {
        let prog = parse_ok("ret 42");
        assert_eq!(prog.len(), 1);
        let Stmt::Ret(ref expr) = prog[0].node else {
            panic!("expected Ret, got {:?}", prog[0].node)
        };
        assert!(matches!(expr.node, Expr::Int(ref s) if s == "42"));
    }

    #[test]
    fn parse_ret_with_semicolon() {
        let prog = parse_ok("ret true;");
        assert_eq!(prog.len(), 1);
        assert!(matches!(prog[0].node, Stmt::Ret(_)));
    }

    #[test]
    fn parse_no_semicolon() {
        // Semicolons are optional; a bare call must parse successfully.
        let prog = parse_ok(r#"print("hello")"#);
        assert_eq!(prog.len(), 1);
    }

    #[test]
    fn parse_let_stmt() {
        let prog = parse_ok("let x = 42");
        assert_eq!(prog.len(), 1);
        let Stmt::Let {
            ref name,
            ref value,
        } = prog[0].node
        else {
            panic!("expected Let, got {:?}", prog[0].node)
        };
        assert_eq!(name, "x");
        assert!(matches!(value.node, Expr::Int(ref s) if s == "42"));
    }

    #[test]
    fn parse_let_with_semicolon() {
        let prog = parse_ok(r#"let s = "hi";"#);
        assert_eq!(prog.len(), 1);
        assert!(matches!(prog[0].node, Stmt::Let { .. }));
    }

    #[test]
    fn parse_with_bare() {
        let prog = parse_ok("with { print(1) }");
        assert_eq!(prog.len(), 1);
        let Stmt::Expr(ref expr) = prog[0].node else {
            panic!()
        };
        let Expr::With {
            ref bindings,
            ref body,
        } = expr.node
        else {
            panic!()
        };
        assert!(bindings.is_empty());
        assert_eq!(body.len(), 1);
    }

    #[test]
    fn parse_with_bindings() {
        let prog = parse_ok("with x = 1, y = 2 { print(x) }");
        assert_eq!(prog.len(), 1);
        let Stmt::Expr(ref expr) = prog[0].node else {
            panic!()
        };
        let Expr::With { ref bindings, .. } = expr.node else {
            panic!()
        };
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].0, "x");
        assert_eq!(bindings[1].0, "y");
    }

    // ── error path ────────────────────────────────────────────────────────────

    #[test]
    fn parse_error_unclosed_paren() {
        parse_err(r#"print("hello";"#);
    }
}
