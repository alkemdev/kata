//! Shared helpers for snapshot tests.

use katars::ks::{self, Interpreter};

/// Run a KS program and return stdout.
pub fn run_ok(source: &str) -> String {
    let prelude_src = include_str!("../../../std/prelude.ks");
    let prelude = ks::parse(prelude_src, "<prelude>").expect("prelude parse failed");

    let mut interp = Interpreter::new();
    let mut out = Vec::new();

    interp
        .exec_program(&prelude, None, &mut out)
        .expect("prelude exec failed");

    let program = ks::parse(source, "<test>").expect("program parse failed");
    interp
        .exec_program(&program, None, &mut out)
        .expect("program exec failed");

    String::from_utf8(out).unwrap()
}

/// Run a KS program and capture the error rendering (ariadne output).
pub fn run_error(source: &str) -> String {
    let prelude_src = include_str!("../../../std/prelude.ks");
    let prelude = ks::parse(prelude_src, "<prelude>").expect("prelude parse failed");
    let filename = "<test>";

    let mut interp = Interpreter::new();
    let mut out = Vec::new();

    interp
        .exec_program(&prelude, None, &mut out)
        .expect("prelude exec failed");

    let program = match ks::parse(source, filename) {
        Ok(p) => p,
        Err(()) => return "parse error (not captured)".to_string(),
    };

    match interp.exec_program(&program, None, &mut out) {
        Ok(()) => "no error".to_string(),
        Err(e) => {
            let message = e.kind.format_with(&interp.types);
            if let Some(span) = e.span.filter(|s| s.1 <= source.len()) {
                use ariadne::{Label, Report, ReportKind, Source};
                let mut buf = Vec::new();
                let mut report =
                    Report::build(ReportKind::Error, filename, span.0).with_message(&message);
                let primary = Label::new((filename, span.0..span.1));
                report = report.with_label(if e.labels.is_empty() {
                    primary.with_message(&message)
                } else {
                    primary
                });
                for (label_span, label_msg) in &e.labels {
                    report = report.with_label(
                        Label::new((filename, label_span.0..label_span.1)).with_message(label_msg),
                    );
                }
                report
                    .with_config(ariadne::Config::default().with_color(false))
                    .finish()
                    .write((filename, Source::from(source)), &mut buf)
                    .unwrap();
                String::from_utf8(buf).unwrap()
            } else {
                format!("Error: {message}\n(no source location)")
            }
        }
    }
}
