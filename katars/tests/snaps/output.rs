//! Snapshot tests for program output.
//!
//! These capture stdout from KS programs. Useful for verifying complex
//! output formatting, display of types, etc.

mod helpers;

#[test]
fn hello_world() {
    insta::assert_snapshot!(helpers::run_ok("print(\"hello, world\")"));
}

#[test]
fn array_display() {
    insta::assert_snapshot!(helpers::run_ok(
        "let a = [1, 2, 3]\nfor x in a { print(x) }"
    ));
}

#[test]
fn opt_display() {
    insta::assert_snapshot!(helpers::run_ok(
        "print(Opt[Int].Val(42))\nprint(Opt[Int].Non)"
    ));
}

#[test]
fn match_expression() {
    insta::assert_snapshot!(helpers::run_ok(
        "let x = match Opt[Int].Val(42) {\n    Val(n) -> n * 2,\n    Non -> 0,\n}\nprint(x)"
    ));
}

#[test]
fn struct_display() {
    insta::assert_snapshot!(helpers::run_ok(
        "kind Point { x: Int, y: Int }\nlet p = Point { x: 1, y: 2 }\nprint(p)"
    ));
}
