use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::{
    input::{Stream, ValueInput},
    pratt::*,
    prelude::*,
};
use logos::Logos;
use tracing::{debug, info};

use super::ast::{
    AssignTarget, AstFieldDef, AstVariantDef, BinOp, Expr, FuncDef, InterpPart, MethodSig, Param,
    Program, Spanned, Stmt, UnaryOp,
};
use super::lexer::{StringPart, Token};

// ── Grammar ───────────────────────────────────────────────────────────────────
//
//   program    = stmt*
//   stmt       = 'import' IDENT ('.' IDENT)* ';'?                    -- module import
//              | 'enum' IDENT type_params? '{' variant_list '}'       -- enum def
//              | 'kind' IDENT type_params? '{' field_list '}'          -- kind def
//              | 'type' IDENT type_params? '{' method_sig* '}'         -- interface def
//              | 'impl' IDENT type_params? ('as' expr)? '{' func_def* '}' -- impl block
//              | 'func' IDENT '(' params? ')' ret_ann? '{' stmt* '}'  -- function def
//              | 'let' IDENT '=' expr ';'?                            -- variable binding
//              | 'break' ';'?                                          -- break out of loop
//              | 'continue' ';'?                                       -- next loop iteration
//              | 'ret' expr ';'?                                       -- explicit return
//              | expr_or_assign ';'?                                   -- expr or assignment
//   type_params = '[' IDENT (',' IDENT)* ']'
//   variant_list = variant (',' variant)* ','?
//   variant    = IDENT '(' expr (',' expr)* ')'    -- data variant (type exprs)
//              | IDENT                               -- unit variant
//   field_list = field (',' field)* ','?
//   field      = IDENT ':' expr                     -- type annotation is a full expression
//   params     = param (',' param)*
//   param      = IDENT ':' expr                     -- typed param (type ann is expr)
//              | IDENT                               -- untyped param
//   ret_ann    = ':' expr                            -- return type annotation (expr)
//   expr       = with_expr | unsafe_expr | if_expr | while_expr | for_expr | op_expr
//   unsafe_expr = 'unsafe' '{' stmt* '}'
//   for_expr   = 'for' IDENT 'in' expr '{' stmt* '}'
//   while_expr = 'while' expr '{' stmt* '}'
//   with_expr  = 'with' (binding (',' binding)*)? '{' stmt* '}'
//   binding    = IDENT '=' expr
//   if_expr    = 'if' expr '{' stmt* '}' ('else' '{' stmt* '}' | 'elif' if_expr)?
//   op_expr    = unary (binop unary)*          -- pratt precedence climbing
//   binop      = '+' | '-' | '*' | '/' | '==' | '!=' | '<' | '>' | '<=' | '>='
//              | '&&' | '||'
//   unary      = ('-' | '!') unary | postfix
//   postfix    = atom ('.' IDENT | '[' args ']' | '(' args ')' | '{' field_init* '}')*
//   field_init = IDENT ':' expr
//   expr_or_assign = expr ('=' expr)?           -- assignment if '=' follows
//   atom       = ident | str | num | 'true' | 'false' | 'nil' | '(' expr ')'
//   str        = '"' (text | escape | '{' expr '}')* '"'
//   escape     = '\n' | '\t' | '\\' | '\"' | '\{' | '\}'

fn span(s: &SimpleSpan) -> (usize, usize) {
    (s.start, s.end)
}

// ── Statement & expression parsers ───────────────────────────────────────────
//
// `with` bodies contain statements, and statements contain expressions, so the
// two are mutually recursive. We use `recursive` at the statement level and
// build the expression parser inline.

/// Parse a KataScript expression from a source fragment (used for string interpolation).
/// Returns `None` if the fragment fails to parse.
fn parse_fragment(source: &str) -> Option<Spanned<Expr>> {
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

    // We need an expression parser. Re-use stmt_parser and expect a single expr statement.
    let parser = stmt_parser().then_ignore(end());
    let (ast, errors) = parser.parse(token_stream).into_output_errors();

    if !errors.is_empty() {
        return None;
    }

    let stmt = ast?;
    match stmt.node {
        Stmt::Expr(expr) => Some(expr),
        _ => None,
    }
}

fn stmt_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Stmt>, extra::Err<Rich<'tokens, Token>>>
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|stmt| {
        // ── expression parser (uses `stmt` for `with` bodies) ────────────

        let expr = recursive(|expr| {
            // ── atoms ────────────────────────────────────────────────

            let str_lit = select! { Token::Str(parts) => parts }.validate(
                |parts: Vec<StringPart>, extra, emitter| {
                    // Single Lit (or empty) → plain Expr::Str. Otherwise → Expr::Interp.
                    let has_interp =
                        parts.iter().any(|p| matches!(p, StringPart::Interp(_)));
                    if !has_interp {
                        // Concatenate all Lit segments into a single string.
                        let s: String = parts
                            .into_iter()
                            .map(|p| match p {
                                StringPart::Lit(s) => s,
                                StringPart::Interp(_) => unreachable!(),
                            })
                            .collect();
                        Expr::Str(s)
                    } else {
                        let ast_parts = parts
                            .into_iter()
                            .map(|p| match p {
                                StringPart::Lit(s) => InterpPart::Lit(s),
                                StringPart::Interp(src) => {
                                    match parse_fragment(&src) {
                                        Some(expr) => InterpPart::Expr(expr),
                                        None => {
                                            emitter.emit(Rich::custom(
                                                extra.span(),
                                                format!(
                                                    "invalid expression in string interpolation: {{{src}}}"
                                                ),
                                            ));
                                            InterpPart::Lit(format!("{{{src}}}"))
                                        }
                                    }
                                }
                            })
                            .collect();
                        Expr::Interp { parts: ast_parts }
                    }
                },
            );
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

            let unsafe_expr = just(Token::Unsafe)
                .ignore_then(
                    stmt.clone()
                        .repeated()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                )
                .map_with(|body, ex| Spanned::new(Expr::Unsafe { body }, span(&ex.span())));

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

            let for_expr = just(Token::For)
                .ignore_then(select! { Token::Ident(name) => name })
                .then_ignore(just(Token::In))
                .then(expr.clone())
                .then(block.clone())
                .map_with(|((binding, iter_expr), body), ex| {
                    Spanned::new(
                        Expr::For {
                            binding,
                            iter_expr: Box::new(iter_expr),
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
                .or(unsafe_expr)
                .or(if_expr)
                .or(while_expr)
                .or(for_expr);

            // ── postfix chain: .attr, [item], (call) ─────────────────

            enum Postfix {
                Attr(String),
                Item(Vec<Spanned<Expr>>),
                Call(Vec<Spanned<Expr>>),
                Construct(Vec<(String, Spanned<Expr>)>),
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

            let field_init = select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Colon))
                .then(expr.clone());

            let construct = field_init
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .map(Postfix::Construct);

            let postfix = attr.or(item).or(call).or(construct);

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
                    Postfix::Construct(fields) => Spanned::new(
                        Expr::Construct {
                            type_expr: Box::new(lhs),
                            fields,
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

        // variant = IDENT '(' expr (',' expr)* ')' | IDENT
        let variant_def = select! { Token::Ident(name) => name }
            .then(
                expr.clone()
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

        // type_params = '[' IDENT (',' IDENT)* ']'
        let type_params = select! { Token::Ident(name) => name }
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket));

        // enum_def = 'enum' IDENT type_params? '{' variant_list '}'
        let enum_def = just(Token::Enum)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(type_params.clone().or_not())
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

        // kind_def = 'kind' IDENT type_params? '{' field_list '}'
        let field_def = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .map(|(name, type_ann)| AstFieldDef { name, type_ann });

        let kind_def = just(Token::Kind)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(type_params.clone().or_not())
            .then(
                field_def
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|((name, type_params), fields), ex| {
                Spanned::new(
                    Stmt::KindDef {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        fields,
                    },
                    span(&ex.span()),
                )
            });

        // param = IDENT ':' expr | IDENT
        let param = select! { Token::Ident(name) => name }
            .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
            .map(|(name, type_ann)| Param { name, type_ann });

        // ret_ann = ':' expr
        let ret_ann = just(Token::Colon).ignore_then(expr.clone());

        // interface_def = 'type' IDENT type_params? '{' method_sig* '}'
        // method_sig = 'func' IDENT '(' params ')' ret_ann?
        let method_sig = just(Token::Func)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(
                param
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(ret_ann.clone().or_not())
            .map(|((name, params), ret_type)| MethodSig {
                name,
                params,
                ret_type,
            });

        let interface_def = just(Token::Type)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(type_params.clone().or_not())
            .then(
                method_sig
                    .separated_by(just(Token::Comma).or_not())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|((name, type_params), methods), ex| {
                Spanned::new(
                    Stmt::InterfaceDef {
                        name,
                        type_params: type_params.unwrap_or_default(),
                        methods,
                    },
                    span(&ex.span()),
                )
            });

        // impl_block = 'impl' IDENT ('as' expr)? '{' func_def* '}'
        let impl_func_def = just(Token::Func)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(
                param
                    .clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(ret_ann.clone().or_not())
            .then(
                stmt.clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(((name, params), ret_type), body), ex| {
                Spanned::new(
                    FuncDef {
                        name,
                        params,
                        ret_type,
                        body,
                    },
                    span(&ex.span()),
                )
            });

        let impl_block = just(Token::Impl)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(type_params.clone().or_not())
            .then(just(Token::As).ignore_then(expr.clone()).or_not())
            .then(
                impl_func_def
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(((type_name, tparams), as_type), methods), ex| {
                Spanned::new(
                    Stmt::Impl {
                        type_name,
                        type_params: tparams.unwrap_or_default(),
                        as_type,
                        methods,
                    },
                    span(&ex.span()),
                )
            });

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
                    Stmt::FuncDef(FuncDef {
                        name,
                        params,
                        ret_type,
                        body,
                    }),
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

        let break_stmt = just(Token::Break)
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|_, ex| Spanned::new(Stmt::Break, span(&ex.span())));

        let continue_stmt = just(Token::Continue)
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|_, ex| Spanned::new(Stmt::Continue, span(&ex.span())));

        let ret_stmt = just(Token::Ret)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|expr, ex| Spanned::new(Stmt::Ret(expr), span(&ex.span())));

        // expr_or_assign: parse expr, then optionally '=' expr for assignment.
        // If '=' follows, convert lhs to AssignTarget.
        let expr_or_assign = expr
            .clone()
            .then(just(Token::Eq).ignore_then(expr).or_not())
            .then_ignore(just(Token::Semicolon).or_not())
            .validate(|(lhs, rhs), extra, emitter| {
                let s = span(&extra.span());
                match rhs {
                    None => Spanned::new(Stmt::Expr(lhs), s),
                    Some(value) => {
                        let target = match lhs.node {
                            Expr::Name(name) => AssignTarget::Name(name),
                            Expr::Attr { object, name } => {
                                AssignTarget::Attr { object, attr: name }
                            }
                            _ => {
                                emitter
                                    .emit(Rich::custom(extra.span(), "invalid assignment target"));
                                AssignTarget::Name("<invalid>".into())
                            }
                        };
                        Spanned::new(Stmt::Assign { target, value }, s)
                    }
                }
            });

        // import_stmt = 'import' IDENT ('.' IDENT)* ('.' '{' IDENT (',' IDENT)* '}')?
        let import_names = just(Token::Dot).ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        );

        let import_stmt = just(Token::Import)
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .separated_by(just(Token::Dot))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(import_names.or_not())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|(path, names), ex| {
                Spanned::new(Stmt::Import { path, names }, span(&ex.span()))
            });

        import_stmt
            .or(enum_def)
            .or(kind_def)
            .or(interface_def)
            .or(impl_block)
            .or(func_def)
            .or(let_stmt)
            .or(break_stmt)
            .or(continue_stmt)
            .or(ret_stmt)
            .or(expr_or_assign)
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
