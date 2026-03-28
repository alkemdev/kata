pub mod ast;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod native;
pub mod numeric;
pub mod parser;
pub mod types;
pub mod value;

pub use ast::Program;
pub use interpreter::Interpreter;
pub use lexer::SpannedToken;

use ariadne::{Color, Label, Report, ReportKind, Source};
use error::RuntimeError;
use types::TypeRegistry;

/// Lex `source` into a token stream. Always succeeds; lex errors appear as
/// `Token::Error` entries in the result.
pub fn lex(source: &str) -> Vec<SpannedToken> {
    lexer::lex(source)
}

/// Parse `source` into an AST. Errors are printed to stderr via ariadne.
pub fn parse(source: &str, filename: &str) -> Result<Program, ()> {
    parser::parse(source, filename)
}

/// The KataScript standard prelude, embedded at compile time.
const PRELUDE_SRC: &str = include_str!("../../../std/prelude.ks");

/// Run `source`: parse then evaluate. Errors go to stderr.
pub fn run(source: &str, filename: &str) -> Result<(), ()> {
    let prelude = parse(PRELUDE_SRC, "<prelude>").map_err(|()| {
        eprintln!("fatal: failed to parse standard prelude");
    })?;
    let program = parse(source, filename)?;
    let mut interp = Interpreter::new();

    // Run the prelude.
    interp
        .exec_program(&prelude, None, &mut std::io::stdout())
        .map_err(|e| {
            // Prelude errors are developer bugs — render against PRELUDE_SRC.
            render_error(&e, &interp.types, PRELUDE_SRC, "<prelude>");
        })?;

    // Run the user program.
    interp
        .exec_program(&program, None, &mut std::io::stdout())
        .map_err(|e| {
            render_error(&e, &interp.types, source, filename);
        })
}

/// Render a RuntimeError to stderr using ariadne (if span is available)
/// or as a plain message (if span-less).
pub fn render_error(err: &RuntimeError, types: &TypeRegistry, source: &str, filename: &str) {
    let message = err.kind.format_with(types);
    // Only render with ariadne if the span is within the source's range.
    // Errors from stdlib code may have spans relative to prelude source.
    if let Some(span) = err.span.filter(|s| s.1 <= source.len()) {
        let mut report = Report::build(ReportKind::Error, filename, span.0).with_message(&message);
        // Only label the primary span with the message when there are no
        // secondary labels. With secondary labels, they provide context
        // and repeating the header is redundant.
        let primary = Label::new((filename, span.0..span.1)).with_color(Color::Red);
        report = report.with_label(if err.labels.is_empty() {
            primary.with_message(&message)
        } else {
            primary
        });

        for (label_span, label_msg) in &err.labels {
            report = report.with_label(
                Label::new((filename, label_span.0..label_span.1))
                    .with_message(label_msg)
                    .with_color(Color::Yellow),
            );
        }

        if let Some(ref help) = err.help {
            report = report.with_help(help);
        }
        if let Some(ref note) = err.note {
            report = report.with_note(note);
        }

        report
            .finish()
            .eprint((filename, Source::from(source)))
            .unwrap();
    } else {
        eprintln!("runtime error: {message}");
    }
}
