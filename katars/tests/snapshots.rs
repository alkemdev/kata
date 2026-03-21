//! Snapshot tests for error message rendering.
//!
//! These capture the full ariadne output (with source context) so we can
//! verify error messages look right. Not part of the conformance suite —
//! other KS interpreters may format differently.
//!
//! Run with: cargo test --test snapshots
//! Review with: cargo insta review

use std::io::Write;

/// Run a KS program and capture stderr (error output).
fn run_error(source: &str) -> String {
    let prelude_src = include_str!("../../std/prelude.ks");
    let prelude = katars::ks::parse(prelude_src, "<prelude>").expect("prelude parse failed");
    let filename = "<test>";

    let mut interp = katars::ks::Interpreter::new();
    let mut out = Vec::new();

    // Run prelude
    interp
        .exec_program(&prelude, None, &mut out)
        .expect("prelude exec failed");

    // Parse user program
    let program = match katars::ks::parse(source, filename) {
        Ok(p) => p,
        Err(()) => {
            // Parse error — capture it
            // For now, just return a placeholder since parse errors
            // go directly to stderr via ariadne, not through our capture.
            return "parse error (not captured)".to_string();
        }
    };

    // Run and capture error
    match interp.exec_program(&program, None, &mut out) {
        Ok(()) => "no error".to_string(),
        Err(e) => {
            let types = &interp.types;
            let message = e.kind.format_with(types);
            if let Some(span) = e.span.filter(|s| s.1 <= source.len()) {
                // Render with ariadne to a string (not stderr)
                use ariadne::{Label, Report, ReportKind, Source};
                let mut buf = Vec::new();
                let mut report = Report::build(ReportKind::Error, filename, span.0)
                    .with_message(&message)
                    .with_label(Label::new((filename, span.0..span.1)).with_message(&message));
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

#[test]
fn error_undefined_variable() {
    let output = run_error("let x = y");
    insta::assert_snapshot!(output);
}

#[test]
fn error_type_mismatch_binop() {
    let output = run_error("let x = 1 + \"a\"");
    insta::assert_snapshot!(output);
}

#[test]
fn error_division_by_zero() {
    let output = run_error("let x = 1 / 0");
    insta::assert_snapshot!(output);
}

#[test]
fn error_interp_undefined() {
    let output = run_error("print(\"hello {x}\")");
    insta::assert_snapshot!(output);
}

#[test]
fn error_no_match_arm() {
    let output = run_error("match 42 { 0 -> \"zero\" }");
    insta::assert_snapshot!(output);
}

#[test]
fn error_try_non_opt() {
    let output = run_error("let x = 42?");
    insta::assert_snapshot!(output);
}

#[test]
fn error_empty_array() {
    let output = run_error("let a = []");
    insta::assert_snapshot!(output);
}

#[test]
fn error_break_outside_loop() {
    let output = run_error("break");
    insta::assert_snapshot!(output);
}

#[test]
fn error_ret_outside_func() {
    let output = run_error("ret 1");
    insta::assert_snapshot!(output);
}

#[test]
fn error_enum_type_mismatch() {
    let output = run_error("Opt[Int].Val(\"wrong\")");
    insta::assert_snapshot!(output);
}

#[test]
fn error_mixed_array() {
    let output = run_error("let a = [1, \"two\", 3]");
    insta::assert_snapshot!(output);
}

#[test]
fn error_unknown_module() {
    let output = run_error("import std.nonexistent");
    insta::assert_snapshot!(output);
}
