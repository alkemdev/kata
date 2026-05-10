use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::{
    input::{Stream, ValueInput},
    pratt::*,
    prelude::*,
};
use logos::Logos;
use tracing::{debug, info};

use super::ast::{
    AssignTarget, AstFieldDef, AstVariantDef, BinInterpPart, BinOp, Expr, FuncDef, InterpPart,
    Literal, MatchArm, MethodSig, Param, Pattern, Program, Span, Spanned, Stmt, TypePattern,
    UnaryOp,
};
use super::lexer::{BinPart, StringPart, Token};

// ── Grammar ───────────────────────────────────────────────────────────────────
//
//   program    = stmt*
//   stmt       = 'import' IDENT ('.' IDENT)* ';'?                    -- module import
//              | 'enum' IDENT type_params? '{' variant_list '}'       -- enum def
//              | 'kind' IDENT type_params? '{' field_list '}'          -- kind def
//              | 'type' IDENT type_params? '{' method_sig* '}'         -- interface def
//              | 'impl' type_pattern ('as' expr)? '{' func_def* '}' -- impl block
//              | 'func' IDENT '(' params? ')' ret_ann? '{' stmt* '}'  -- function def
//              | 'let' IDENT '=' expr ';'?                            -- variable binding
//              | 'bail' ';'?                                           -- exit current loop
//              | 'cont' ';'?                                           -- next loop iteration
//              | 'ret' expr ';'?                                       -- explicit return
//              | expr_or_assign ';'?                                   -- expr or assignment
//   type_params = '[' IDENT (',' IDENT)* ']'
//   type_pattern = '@' IDENT                                   -- binding (@T)
//                | IDENT '[' type_pattern (',' type_pattern)* ']' -- apply (Arr[@T])
//                | IDENT                                        -- concrete (Int)
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
//   binop      = '+' | '-' | '*' | '/' | '%' | '==' | '!=' | '<' | '>' | '<=' | '>='
//              | '&&' | '||'
//   unary      = ('-' | '!') unary | postfix
//   postfix    = atom ('.' IDENT | '.' NUM | '[' args ']' | '(' args ')' | '{' field_init* '}' | '?' | '!')*
//                                  -- '.' NUM is positional tuple field access (`t.0`, `t.0.1`).
//                                  -- The lexer doesn't fuse N.M into one Num — that's a parser
//                                  -- decision, made in atom position (float literal merge) vs
//                                  -- postfix position (consecutive tuple indices).
//   field_init = IDENT ':' expr
//   expr_or_assign = expr ('=' expr)?           -- assignment if '=' follows
//   atom       = ident | str | num | 'true' | 'false' | 'nil' | tup_or_group | arr_lit | func_expr
//   func_expr  = 'func' '(' params ')' (':' expr)? '{' stmt* '}'  -- anonymous closure
//   tup_or_group = '(' ')' | '(' expr ')' | '(' expr ',' ')' | '(' expr (',' expr)+ ','? ')'
//   arr_lit    = '[' (expr (',' expr)* ','?)? ']'
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
/// Parse a fragment of source code (e.g., from string interpolation).
/// `base_offset` is the byte position of the fragment in the original source,
/// used to offset spans so error messages point to the right location.
fn parse_fragment(source: &str, base_offset: usize) -> Option<Spanned<Expr>> {
    let token_iter =
        Token::lexer(source)
            .spanned()
            .map(move |(result, span): (_, std::ops::Range<usize>)| {
                let tok = result.unwrap_or(Token::Error);
                // Offset spans by base_offset so they map to the original source.
                let adjusted = (span.start + base_offset)..(span.end + base_offset);
                (tok, SimpleSpan::from(adjusted))
            });

    let eoi = source.len() + base_offset;
    let token_stream =
        Stream::from_iter(token_iter).map(SimpleSpan::from(eoi..eoi), |(t, s): (_, _)| (t, s));

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
        // ── pattern parser (hoisted out of expr so let/for can use it) ───
        //
        // pattern    = IDENT '(' pattern (',' pattern)* ','? ')'    -- variant
        //            | tup_pat                                       -- tuple
        //            | '_'                                            -- wildcard
        //            | literal                                        -- 42, -42, 3.14, "hello", true, false, nil
        //            | IDENT                                          -- binding
        // tup_pat    = '(' ')' | '(' pat ',' ')' | '(' pat (',' pat)+ ','? ')'
        // literal    = '-'? NUM | STR | 'true' | 'false' | 'nil'

        let pattern = recursive(|pat| {
            // Variant: IDENT '(' pat (',' pat)* ','? ')'
            let variant_with_bindings = select! { Token::Ident(name) => name }
                .map_with(|name, ex| Spanned::new(name, span(&ex.span())))
                .then(
                    pat.clone()
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LParen), just(Token::RParen)),
                )
                .map_with(|(name, bindings), ex| {
                    Spanned::new(Pattern::Variant { name, bindings }, span(&ex.span()))
                });

            // Tuple: same disambiguation as tup_or_group expressions.
            // (p) is grouping (returns inner pattern); (p,) is 1-tuple; (p, q) is 2-tuple.
            let tuple_pat = just(Token::LParen)
                .ignore_then(
                    pat.clone()
                        .separated_by(just(Token::Comma))
                        .collect::<Vec<_>>(),
                )
                .then(just(Token::Comma).or_not())
                .then_ignore(just(Token::RParen))
                .map_with(|(elements, trailing_comma), ex| {
                    let s = span(&ex.span());
                    if elements.len() == 1 && trailing_comma.is_none() {
                        // (p) — grouping, return the inner pattern unchanged
                        elements.into_iter().next().unwrap()
                    } else {
                        Spanned::new(Pattern::Tuple(elements), s)
                    }
                });

            // Wildcard: _ (lexes as an identifier)
            let wildcard = select! { Token::Ident(name) => name }
                .filter(|n| n == "_")
                .map_with(|_, ex| Spanned::new(Pattern::Wildcard, span(&ex.span())));

            // Literal: '-'? NUM ('.' NUM)? | "hello" | true | false | nil
            // Sign is absorbed into the literal so `-42` is
            // `Literal::Int("-42")`, not UnaryOp(Neg, 42). The lexer no
            // longer fuses N.M into a single Num — we merge here so float
            // patterns (`3.14 -> ...`) work the same as float expressions.
            let lit_num = just(Token::Minus)
                .or_not()
                .then(select! { Token::Num(s) => s })
                .then(
                    just(Token::Dot)
                        .ignore_then(select! { Token::Num(f) => f })
                        .or_not(),
                )
                .try_map(|((neg, int_part), frac_part), span| {
                    let prefix = if neg.is_some() { "-" } else { "" };
                    let lit = match frac_part {
                        Some(frac) => {
                            let decimal = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
                            if !decimal(&int_part) || !decimal(&frac) {
                                return Err(Rich::custom(
                                    span,
                                    format!("invalid float literal '{int_part}.{frac}'"),
                                ));
                            }
                            Literal::Float(format!("{prefix}{int_part}.{frac}"))
                        }
                        None => Literal::Int(format!("{prefix}{int_part}")),
                    };
                    Ok(lit)
                })
                .map_with(|lit, ex| {
                    let s_pan = span(&ex.span());
                    Spanned::new(Pattern::Literal(Spanned::new(lit, s_pan)), s_pan)
                });
            let lit_str = select! { Token::Str(parts) => parts }
                .try_map(|parts, _span| {
                    // Only plain string literals (no interpolation) in patterns
                    if parts.len() == 1 {
                        if let super::lexer::StringPart::Lit(s) = &parts[0] {
                            return Ok(s.clone());
                        }
                    }
                    if parts.is_empty() {
                        return Ok(String::new());
                    }
                    Err(Rich::custom(
                        _span,
                        "interpolated strings not allowed in match patterns",
                    ))
                })
                .map_with(|s, ex| {
                    let s_pan = span(&ex.span());
                    Spanned::new(
                        Pattern::Literal(Spanned::new(Literal::Str(s), s_pan)),
                        s_pan,
                    )
                });
            let lit_true = just(Token::True).map_with(|_, ex| {
                let s_pan = span(&ex.span());
                Spanned::new(
                    Pattern::Literal(Spanned::new(Literal::Bool(true), s_pan)),
                    s_pan,
                )
            });
            let lit_false = just(Token::False).map_with(|_, ex| {
                let s_pan = span(&ex.span());
                Spanned::new(
                    Pattern::Literal(Spanned::new(Literal::Bool(false), s_pan)),
                    s_pan,
                )
            });
            let lit_nil = just(Token::Nil).map_with(|_, ex| {
                let s_pan = span(&ex.span());
                Spanned::new(
                    Pattern::Literal(Spanned::new(Literal::Nil, s_pan)),
                    s_pan,
                )
            });

            // Bare ident: unit variant or catch-all binding
            let bare_ident = select! { Token::Ident(name) => name }
                .filter(|n| n != "_")
                .map_with(|name, ex| {
                    // Could be unit variant or binding — resolved at runtime
                    let s_pan = span(&ex.span());
                    Spanned::new(Pattern::Binding(Spanned::new(name, s_pan)), s_pan)
                });

            // Priority: variant (IDENT before paren) > tuple (paren) > wildcard > literal > binding
            variant_with_bindings
                .or(tuple_pat)
                .or(wildcard)
                .or(lit_num)
                .or(lit_str)
                .or(lit_true)
                .or(lit_false)
                .or(lit_nil)
                .or(bare_ident)
        });

        // ── expression parser (uses `stmt` for `with` bodies) ────────────

        let expr = recursive(|expr| {
            // ── atoms ────────────────────────────────────────────────

            let str_lit = select! { Token::Str(parts) => parts }.validate(
                |parts: Vec<StringPart>, extra, emitter| {
                    // Single Lit (or empty) → plain Expr::Str. Otherwise → Expr::Interp.
                    let has_interp =
                        parts.iter().any(|p| matches!(p, StringPart::Interp(_, _)));
                    if !has_interp {
                        // Concatenate all Lit segments into a single string.
                        let s: String = parts
                            .into_iter()
                            .map(|p| match p {
                                StringPart::Lit(s) => s,
                                StringPart::Interp(_, _) => unreachable!(),
                            })
                            .collect();
                        Expr::Str(s)
                    } else {
                        let ast_parts = parts
                            .into_iter()
                            .map(|p| match p {
                                StringPart::Lit(s) => InterpPart::Lit(s),
                                StringPart::Interp(src, offset) => {
                                    match parse_fragment(&src, offset) {
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
            let bin_lit = select! { Token::BinStr(parts) => parts }.validate(
                |parts: Vec<BinPart>, extra, emitter| {
                    let has_interp =
                        parts.iter().any(|p| matches!(p, BinPart::Interp(_, _)));
                    if !has_interp {
                        let bytes: Vec<u8> = parts
                            .into_iter()
                            .flat_map(|p| match p {
                                BinPart::Bytes(bs) => bs,
                                BinPart::Interp(_, _) => unreachable!(),
                            })
                            .collect();
                        Expr::BinLit(bytes)
                    } else {
                        let ast_parts = parts
                            .into_iter()
                            .map(|p| match p {
                                BinPart::Bytes(bs) => BinInterpPart::Bytes(bs),
                                BinPart::Interp(src, offset) => {
                                    match parse_fragment(&src, offset) {
                                        Some(expr) => BinInterpPart::Expr(expr),
                                        None => {
                                            emitter.emit(Rich::custom(
                                                extra.span(),
                                                format!(
                                                    "invalid expression in byte string interpolation: {{{src}}}"
                                                ),
                                            ));
                                            BinInterpPart::Bytes(format!("{{{src}}}").into_bytes())
                                        }
                                    }
                                }
                            })
                            .collect();
                        Expr::BinInterp { parts: ast_parts }
                    }
                },
            );
            // `42`, `0xff`, `0b101` — pure integer (any base).
            // `3.14` — `Num Dot Num` merged into a float here at atom
            // position. The lexer doesn't know whether a `.` is a decimal
            // point or a postfix; the parser does, because here we're
            // committing to a literal at the start of an atom. Both halves
            // must be plain decimal — hex/binary literals don't have a
            // fractional form.
            let num_lit = select! { Token::Num(n) => n }
                .then(
                    just(Token::Dot)
                        .ignore_then(select! { Token::Num(f) => f })
                        .or_not(),
                )
                .try_map(|(int_part, frac_part), span| match frac_part {
                    Some(frac) => {
                        let decimal = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
                        if !decimal(&int_part) || !decimal(&frac) {
                            return Err(Rich::custom(
                                span,
                                format!("invalid float literal '{int_part}.{frac}'"),
                            ));
                        }
                        Ok(Expr::Float(format!("{int_part}.{frac}")))
                    }
                    None => Ok(Expr::Int(int_part)),
                });
            let bool_lit = select! {
                Token::True  => Expr::Bool(true),
                Token::False => Expr::Bool(false),
            };
            let nil_lit = just(Token::Nil).to(Expr::Nil);
            let name = select! { Token::Ident(s) => Expr::Name(s) }
                .or(just(Token::SelfValue).to(Expr::Name("self".to_string())))
                .or(just(Token::SelfType).to(Expr::Name("Self".to_string())));

            // Tuple literal or grouping: () | (expr,) | (expr, expr, ...) | (expr)
            // () = empty tuple, (expr) = grouping, (expr,) = 1-tuple, (a, b) = 2-tuple
            let paren = just(Token::LParen)
                .then(
                    expr.clone()
                        .separated_by(just(Token::Comma))
                        .collect::<Vec<_>>(),
                )
                .then(just(Token::Comma).or_not())
                .then_ignore(just(Token::RParen))
                .map_with(|((_, elements), trailing_comma), ex| {
                    let s = span(&ex.span());
                    if elements.len() == 1 && trailing_comma.is_none() {
                        // (expr) — bare grouping, not a tuple
                        elements.into_iter().next().unwrap()
                    } else {
                        // (), (expr,), (a, b), (a, b, c) — tuple literal
                        Spanned::new(Expr::TupLit { elements }, s)
                    }
                });

            // arr_lit = '[' (expr (',' expr)* ','?)? ']'
            let arr_lit = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map_with(|elements, ex| Spanned::new(Expr::ArrLit { elements }, span(&ex.span())));

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

            // match_expr = 'match' expr '{' match_arm (',' match_arm)* ','? '}'
            // match_arm  = pattern '->' expr  |  pattern '->' '{' stmt* '}'

            // Arm body: single expression (as a statement) or block
            let arm_body = stmt
                .clone()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace))
                .or(stmt.clone().map(|s| vec![s]));

            let match_arm = pattern
                .clone()
                .then_ignore(just(Token::Arrow))
                .then(arm_body)
                .map(|(pattern, body)| MatchArm { pattern, body });

            let match_expr = just(Token::Match)
                .map_with(|_, ex| span(&ex.span()))
                .then(expr.clone())
                .then(
                    match_arm
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                )
                .map_with(|((keyword, subject), arms), ex| {
                    Spanned::new(
                        Expr::Match {
                            keyword,
                            subject: Box::new(subject),
                            arms,
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

            let for_expr = just(Token::For)
                .ignore_then(pattern.clone())
                .then_ignore(just(Token::In))
                .then(expr.clone())
                .then(block.clone())
                .map_with(|((pattern, iter_expr), body), ex| {
                    Spanned::new(
                        Expr::For {
                            pattern,
                            iter_expr: Box::new(iter_expr),
                            body,
                        },
                        span(&ex.span()),
                    )
                });

            // Anonymous function expression: `func(params) (: ret)? { body }`
            // Param parsing is inlined here; the statement-level func_def
            // has its own param parser further down.
            let func_expr_param_name = select! { Token::Ident(name) => name }
                .or(just(Token::SelfValue).to("self".to_string()))
                .map_with(|name, ex| Spanned::new(name, span(&ex.span())));
            let func_expr_param = func_expr_param_name
                .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
                .map(|(name, type_ann)| Param { name, type_ann });
            let func_expr = just(Token::Func)
                .ignore_then(
                    func_expr_param
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LParen), just(Token::RParen)),
                )
                .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
                .then(
                    stmt.clone()
                        .repeated()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                )
                .map_with(|((params, ret_type), body), ex| {
                    Spanned::new(
                        Expr::FuncExpr {
                            params,
                            ret_type: ret_type.map(Box::new),
                            body,
                        },
                        span(&ex.span()),
                    )
                });

            let atom = str_lit
                .or(bin_lit)
                .or(num_lit)
                .or(bool_lit)
                .or(nil_lit)
                .or(name)
                .map_with(|e, ex| Spanned::new(e, span(&ex.span())))
                .or(paren)
                .or(arr_lit)
                .or(with_expr)
                .or(unsafe_expr)
                .or(match_expr)
                .or(if_expr)
                .or(while_expr)
                .or(for_expr)
                .or(func_expr);

            // ── postfix chain: .attr, [item], (call) ─────────────────

            // Each postfix carries (data, end_position) so the foldl can
            // compute the full span from lhs.start to postfix.end.
            #[derive(Clone)]
            enum Postfix {
                Attr(String, Span, usize), // name, name_span, end
                TupIdx(u32, Span, usize),  // idx, idx_span, end
                Item(Vec<Spanned<Expr>>, usize),
                Call(Vec<Spanned<Expr>>, Span, usize), // args, args_span, end
                Construct(Vec<(String, Spanned<Expr>)>, Span, usize), // fields, brace_span, end
                Ques(usize),
                Bang(usize),
            }

            impl Postfix {
                fn end(&self) -> usize {
                    match self {
                        Postfix::Attr(_, _, e)
                        | Postfix::TupIdx(_, _, e)
                        | Postfix::Item(_, e)
                        | Postfix::Call(_, _, e)
                        | Postfix::Construct(_, _, e)
                        | Postfix::Ques(e)
                        | Postfix::Bang(e) => *e,
                    }
                }
            }

            // After `.`: either an identifier (attribute / method) or a Num
            // (tuple positional index). One Num → one TupIdx — no string
            // splitting, because the lexer no longer fuses `N.M` into a
            // single token. `t.0.1` is now `Ident Dot Num Dot Num`, and we
            // pick up `.Num` twice in the postfix chain.
            #[derive(Clone)]
            enum DotPart {
                Ident(String),
                Num(String),
            }
            let dot_postfix = just(Token::Dot)
                .ignore_then(
                    select! {
                        Token::Ident(name) => DotPart::Ident(name),
                        Token::Num(n) => DotPart::Num(n),
                    }
                    .map_with(|p, ex| (p, span(&ex.span()))),
                )
                .try_map(|(part, sp), full_span| match part {
                    DotPart::Ident(name) => Ok(Postfix::Attr(name, sp, sp.1)),
                    DotPart::Num(text) => {
                        // Tuple field indices are plain decimal — reject
                        // hex/binary forms and any non-digit content.
                        let idx: u32 = text.parse().map_err(|_| {
                            Rich::custom(
                                full_span,
                                format!(
                                    "tuple field index must be a decimal integer, got '{text}'"
                                ),
                            )
                        })?;
                        Ok(Postfix::TupIdx(idx, sp, sp.1))
                    }
                });

            let item = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map_with(|args, ex| Postfix::Item(args, span(&ex.span()).1));

            let call = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .map_with(|args, ex| {
                    let s = span(&ex.span());
                    Postfix::Call(args, s, s.1)
                });

            let field_init = select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Colon))
                .then(expr.clone());

            let construct = just(Token::LBrace)
                .map_with(|_, ex| span(&ex.span()))
                .then(
                    field_init
                        .separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then_ignore(just(Token::RBrace))
                .map_with(|(brace_span, fields), ex| {
                    Postfix::Construct(fields, brace_span, span(&ex.span()).1)
                });

            let ques_op = just(Token::Ques).map_with(|_, ex| Postfix::Ques(span(&ex.span()).1));
            let bang_op = just(Token::Bang).map_with(|_, ex| Postfix::Bang(span(&ex.span()).1));

            let postfix = dot_postfix
                .or(item)
                .or(call)
                .or(construct)
                .or(ques_op)
                .or(bang_op);

            let postfix_chain = atom.foldl(postfix.repeated(), |lhs, op| {
                let s = (lhs.span.0, op.end()); // full span: lhs start to postfix end
                match op {
                    Postfix::Attr(name, name_span, _) => Spanned::new(
                        Expr::Attr {
                            object: Box::new(lhs),
                            name,
                            name_span,
                        },
                        s,
                    ),
                    Postfix::TupIdx(idx, idx_span, _) => Spanned::new(
                        Expr::TupIdx {
                            object: Box::new(lhs),
                            idx,
                            idx_span,
                        },
                        s,
                    ),
                    Postfix::Item(args, _) => Spanned::new(
                        Expr::Item {
                            object: Box::new(lhs),
                            args,
                        },
                        s,
                    ),
                    Postfix::Call(args, args_span, _) => Spanned::new(
                        Expr::Call {
                            callee: Box::new(lhs),
                            args,
                            args_span,
                        },
                        s,
                    ),
                    Postfix::Construct(fields, brace_span, _) => Spanned::new(
                        Expr::Construct {
                            type_expr: Box::new(lhs),
                            fields,
                            open_brace: brace_span,
                        },
                        s,
                    ),
                    Postfix::Ques(_) => Spanned::new(Expr::Ques(Box::new(lhs)), s),
                    Postfix::Bang(_) => Spanned::new(Expr::Bang(Box::new(lhs)), s),
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
                infix(left(5), just(Token::Percent), bin!(BinOp::Mod)),
                // Additive
                infix(left(4), just(Token::Plus), bin!(BinOp::Add)),
                infix(left(4), just(Token::Minus), bin!(BinOp::Sub)),
                // As (interface view)
                infix(
                    left(3),
                    just(Token::As),
                    |l: Spanned<Expr>, _, r: Spanned<Expr>, _: &mut _| {
                        let s = (l.span.0, r.span.1);
                        Spanned::new(
                            Expr::As {
                                value: Box::new(l),
                                target: Box::new(r),
                            },
                            s,
                        )
                    },
                ),
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
            .map_with(|name, ex| Spanned::new(name, span(&ex.span())))
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
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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
            .map_with(|name, ex| Spanned::new(name, span(&ex.span())))
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .map(|(name, type_ann)| AstFieldDef { name, type_ann });

        let kind_def = just(Token::Kind)
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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

        // param = (IDENT | 'self') (':' expr)?
        let param_name = select! { Token::Ident(name) => name }
            .or(just(Token::SelfValue).to("self".to_string()))
            .map_with(|name, ex| Spanned::new(name, span(&ex.span())));
        let param = param_name
            .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
            .map(|(name, type_ann)| Param { name, type_ann });

        // ret_ann = ':' expr
        let ret_ann = just(Token::Colon).ignore_then(expr.clone());

        // interface_def = 'type' IDENT type_params? '{' method_sig* '}'
        // method_sig = 'func' IDENT '(' params ')' ret_ann?
        let method_sig = just(Token::Func)
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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

        // Type expression for impl-as: Name or Name[Args].
        // Restricted parser that doesn't go through pratt (avoids `as` conflict).
        let type_expr_simple = select! { Token::Ident(name) => name }
            .map_with(|name, ex| Spanned::new(Expr::Name(name), span(&ex.span())))
            .then(
                expr.clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket))
                    .or_not(),
            )
            .map_with(|(name, args), ex| match args {
                None => name,
                Some(args) => {
                    let s = span(&ex.span());
                    Spanned::new(
                        Expr::Item {
                            object: Box::new(name),
                            args,
                        },
                        s,
                    )
                }
            });

        // type_pattern = '@' IDENT | IDENT '[' type_pattern,* ']' | IDENT
        let type_pattern = recursive(|tp| {
            let binding = just(Token::At)
                .ignore_then(select! { Token::Ident(name) => name })
                .map_with(|name, ex| Spanned::new(TypePattern::Binding(name), span(&ex.span())));

            let concrete_or_apply = select! { Token::Ident(name) => name }
                .then(
                    tp.separated_by(just(Token::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just(Token::LBracket), just(Token::RBracket))
                        .or_not(),
                )
                .map_with(|(name, args), ex| {
                    let s = span(&ex.span());
                    match args {
                        None => Spanned::new(TypePattern::Concrete(name), s),
                        Some(args) => Spanned::new(TypePattern::Apply { base: name, args }, s),
                    }
                });

            binding.or(concrete_or_apply)
        });

        let impl_block = just(Token::Impl)
            .ignore_then(type_pattern)
            .then(just(Token::As).ignore_then(type_expr_simple).or_not())
            .then(
                impl_func_def
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|((target, as_type), methods), ex| {
                Spanned::new(
                    Stmt::Impl {
                        target,
                        as_type,
                        methods,
                    },
                    span(&ex.span()),
                )
            });

        let func_def = just(Token::Func)
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, ex| Spanned::new(name, span(&ex.span()))),
            )
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
            .ignore_then(pattern.clone())
            .then(just(Token::Colon).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|((pattern, type_ann), value), ex| {
                Spanned::new(
                    Stmt::Let {
                        pattern,
                        type_ann,
                        value,
                    },
                    span(&ex.span()),
                )
            });

        let bail_stmt = just(Token::Bail)
            .map_with(|_, ex| span(&ex.span())) // capture keyword span
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|keyword, ex| Spanned::new(Stmt::Bail { keyword }, span(&ex.span())));

        let cont_stmt = just(Token::Cont)
            .map_with(|_, ex| span(&ex.span()))
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|keyword, ex| Spanned::new(Stmt::Cont { keyword }, span(&ex.span())));

        let ret_stmt = just(Token::Ret)
            .map_with(|_, ex| span(&ex.span())) // capture keyword span
            .then(expr.clone())
            .then_ignore(just(Token::Semicolon).or_not())
            .map_with(|(keyword, value), ex| {
                Spanned::new(Stmt::Ret { keyword, value }, span(&ex.span()))
            });

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
                            Expr::Attr { object, name, .. } => {
                                AssignTarget::Attr { object, attr: name }
                            }
                            Expr::Item { object, args } => AssignTarget::Item { object, args },
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
        let spanned_ident = select! { Token::Ident(name) => name }
            .map_with(|name, ex| Spanned::new(name, span(&ex.span())));

        let import_names = just(Token::Dot).ignore_then(
            spanned_ident
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        );

        let import_stmt = just(Token::Import)
            .ignore_then(
                spanned_ident
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
            .or(bail_stmt)
            .or(cont_stmt)
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
/// Parse without printing errors (for tab completion, etc.).
pub fn parse_silent(source: &str) -> Result<Program, ()> {
    let spanned_tokens = super::lexer::lex(source);
    let token_iter = spanned_tokens
        .into_iter()
        .map(|st| (st.token, SimpleSpan::from(st.start..st.end)));
    let token_stream = Stream::from_iter(token_iter).map(
        SimpleSpan::from(source.len()..source.len()),
        |(t, s): (_, _)| (t, s),
    );
    let (ast, errors) = program_parser().parse(token_stream).into_output_errors();
    if errors.is_empty() {
        ast.ok_or(())
    } else {
        Err(())
    }
}

pub fn parse(source: &str, filename: &str) -> Result<Program, ()> {
    info!(filename, bytes = source.len(), "parsing");

    // Lex via the full pipeline (includes post-lex rewrites like float splitting).
    let spanned_tokens = super::lexer::lex(source);
    let token_iter = spanned_tokens
        .into_iter()
        .map(|st| (st.token, SimpleSpan::from(st.start..st.end)));
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
            ..
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
        let Stmt::Ret { ref value, .. } = prog[0].node else {
            panic!("expected Ret, got {:?}", prog[0].node)
        };
        assert!(matches!(value.node, Expr::Int(ref s) if s == "42"));
    }

    #[test]
    fn parse_ret_with_semicolon() {
        let prog = parse_ok("ret true;");
        assert_eq!(prog.len(), 1);
        assert!(matches!(prog[0].node, Stmt::Ret { .. }));
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
            ref pattern,
            ref value,
            ..
        } = prog[0].node
        else {
            panic!("expected Let, got {:?}", prog[0].node)
        };
        let Pattern::Binding(ref name) = pattern.node else {
            panic!("expected Binding pattern, got {:?}", pattern.node)
        };
        assert_eq!(name.node, "x");
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
