//! Snapshot tests for error message rendering.
//!
//! Each test captures the full ariadne output so we can verify errors
//! point to the right source location with clear messages.

mod helpers;

#[test]
fn undefined_variable() {
    insta::assert_snapshot!(helpers::run_error("let x = y"));
}

#[test]
fn type_mismatch_binop() {
    insta::assert_snapshot!(helpers::run_error("let x = 1 + \"a\""));
}

#[test]
fn division_by_zero() {
    insta::assert_snapshot!(helpers::run_error("let x = 1 / 0"));
}

#[test]
fn interp_undefined() {
    insta::assert_snapshot!(helpers::run_error("print(\"hello {x}\")"));
}

#[test]
fn no_match_arm() {
    insta::assert_snapshot!(helpers::run_error("match 42 { 0 -> \"zero\" }"));
}

#[test]
fn try_non_opt() {
    insta::assert_snapshot!(helpers::run_error("let x = 42?"));
}

#[test]
fn empty_array() {
    insta::assert_snapshot!(helpers::run_error("let a = []"));
}

#[test]
fn break_outside_loop() {
    insta::assert_snapshot!(helpers::run_error("break"));
}

#[test]
fn ret_outside_func() {
    insta::assert_snapshot!(helpers::run_error("ret 1"));
}

#[test]
fn enum_type_mismatch() {
    insta::assert_snapshot!(helpers::run_error("Opt[Int].Val(\"wrong\")"));
}

#[test]
fn mixed_array() {
    insta::assert_snapshot!(helpers::run_error("let a = [1, \"two\", 3]"));
}

#[test]
fn unknown_module() {
    insta::assert_snapshot!(helpers::run_error("import std.nonexistent"));
}
