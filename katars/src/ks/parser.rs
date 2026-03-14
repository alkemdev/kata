use ariadne::{Color, Label, Report, ReportKind, Source};
use chumsky::{
    input::{Stream, ValueInput},
    prelude::*,
};
use logos::Logos;
use tracing::{debug, info};

use super::ast::{Expr, Program, Spanned, Stmt};
use super::lexer::Token;

// ── Grammar ───────────────────────────────────────────────────────────────────
//
//   program = stmt*
//   stmt    = expr ';'
//   expr    = call
//   call    = ident '(' (expr (',' expr)*)? ')'   -- function call
//           | atom
//   atom    = ident | str | num | 'true' | 'false' | 'nil' | '(' expr ')'
//
// Phase 1 supports only: print("hello, world");
// The full token set is already defined in the lexer for future phases.

fn span(s: &SimpleSpan) -> (usize, usize) {
    (s.start, s.end)
}

// ── Expression parser ─────────────────────────────────────────────────────────

fn expr_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Expr>, extra::Err<Rich<'tokens, Token>>>
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    recursive(|expr| {
        // Atoms: terminals that are not calls.
        let str_lit = select! { Token::Str(s) => Expr::Str(s) };
        let num_lit = select! { Token::Num(n) => Expr::Num(n.parse::<f64>().unwrap_or(0.0)) };
        let bool_lit = select! {
            Token::True  => Expr::Bool(true),
            Token::False => Expr::Bool(false),
        };
        let nil_lit = just(Token::Nil).to(Expr::Nil);

        // Parenthesised expression.
        let paren = expr
            .clone()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        // Ident-or-call: parse the ident, then optionally an argument list.
        // If '(' follows we have a call; otherwise a variable reference.
        let ident_or_call = select! { Token::Ident(name) => name }
            .then(
                expr.clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .or_not(),
            )
            .map_with(|(name, args), ex| {
                let s = span(&ex.span());
                match args {
                    Some(args) => Spanned::new(Expr::Call { callee: name, args }, s),
                    None => Spanned::new(Expr::Ident(name), s),
                }
            });

        let atom = str_lit
            .or(num_lit)
            .or(bool_lit)
            .or(nil_lit)
            .map_with(|e, ex| Spanned::new(e, span(&ex.span())))
            .or(paren)
            .or(ident_or_call);

        atom
    })
}

// ── Statement parser ──────────────────────────────────────────────────────────

fn stmt_parser<'tokens, I>(
) -> impl Parser<'tokens, I, Spanned<Stmt>, extra::Err<Rich<'tokens, Token>>>
where
    I: ValueInput<'tokens, Token = Token, Span = SimpleSpan>,
{
    expr_parser()
        .then_ignore(just(Token::Semicolon))
        .map_with(|expr, ex| Spanned::new(Stmt::Expr(expr), span(&ex.span())))
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
        let Stmt::Expr(ref expr) = prog[0].node;
        let Expr::Call {
            ref callee,
            ref args,
        } = expr.node
        else {
            panic!("expected Call, got {:?}", expr.node);
        };
        assert_eq!(callee, "print");
        assert_eq!(args.len(), 1);
        assert!(matches!(args[0].node, Expr::Str(ref s) if s == "hello, world"));
    }

    #[test]
    fn parse_multiple_statements() {
        let prog = parse_ok(r#"print("a"); print("b");"#);
        assert_eq!(prog.len(), 2);
    }

    #[test]
    fn parse_bool_literals() {
        let prog = parse_ok("print(true); print(false);");
        let Stmt::Expr(ref a) = prog[0].node;
        let Expr::Call { ref args, .. } = a.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Bool(true)));

        let Stmt::Expr(ref b) = prog[1].node;
        let Expr::Call { ref args, .. } = b.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Bool(false)));
    }

    #[test]
    fn parse_nil_literal() {
        let prog = parse_ok("print(nil);");
        let Stmt::Expr(ref expr) = prog[0].node;
        let Expr::Call { ref args, .. } = expr.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Nil));
    }

    #[test]
    fn parse_number_literal() {
        let prog = parse_ok("print(42);");
        let Stmt::Expr(ref expr) = prog[0].node;
        let Expr::Call { ref args, .. } = expr.node else {
            panic!()
        };
        assert!(matches!(args[0].node, Expr::Num(n) if (n - 42.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_span_covers_statement() {
        let prog = parse_ok(r#"print("hi");"#);
        // Statement span should start at 0 and end at or past the semicolon.
        let (start, end) = prog[0].span;
        assert_eq!(start, 0);
        assert!(end > 0);
    }

    // ── error path ────────────────────────────────────────────────────────────

    #[test]
    fn parse_error_missing_semicolon() {
        parse_err(r#"print("hello")"#);
    }

    #[test]
    fn parse_error_unclosed_paren() {
        parse_err(r#"print("hello";"#);
    }
}
