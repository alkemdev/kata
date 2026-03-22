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

#[test]
fn func_param_type_mismatch() {
    insta::assert_snapshot!(helpers::run_error("func f(x: Int) { ret x }\nf(\"wrong\")"));
}

#[test]
fn func_wrong_arity() {
    insta::assert_snapshot!(helpers::run_error(
        "func f(a: Int, b: Int) { ret a + b }\nf(1)"
    ));
}

#[test]
fn struct_field_type_mismatch() {
    insta::assert_snapshot!(helpers::run_error("kind P { x: Int }\nP { x: \"wrong\" }"));
}

#[test]
fn struct_missing_field() {
    insta::assert_snapshot!(helpers::run_error("kind P { x: Int, y: Int }\nP { x: 1 }"));
}

#[test]
fn no_such_attr() {
    insta::assert_snapshot!(helpers::run_error(
        "kind P { x: Int }\nlet p = P { x: 1 }\np.z"
    ));
}

#[test]
fn undefined_func() {
    insta::assert_snapshot!(helpers::run_error("foo(1, 2)"));
}

#[test]
fn variant_wrong_arity() {
    insta::assert_snapshot!(helpers::run_error("Opt[Int].Val(1, 2)"));
}

#[test]
fn continue_outside_loop() {
    insta::assert_snapshot!(helpers::run_error("continue"));
}

#[test]
fn unsafe_required() {
    insta::assert_snapshot!(helpers::run_error("std.mem.alloc(4)"));
}

#[test]
fn use_after_free() {
    insta::assert_snapshot!(helpers::run_error(
        "let raw = unsafe { std.mem.alloc(4) }\nunsafe { std.mem.free(raw) }\nunsafe { std.mem.read(raw, 0) }"
    ));
}

// ── Index errors ────────────────────────────────────────────────

#[test]
fn index_out_of_bounds() {
    insta::assert_snapshot!(helpers::run_error("let a = [10, 20, 30]\nprint(a[5])"));
}

#[test]
fn index_not_indexable() {
    insta::assert_snapshot!(helpers::run_error("let x = 42\nx[0]"));
}

#[test]
fn index_set_out_of_bounds() {
    insta::assert_snapshot!(helpers::run_error("let a = [10, 20, 30]\na[5] = 99"));
}

// ── Postfix span tests ──────────────────────────────────────────
#[test]
fn unknown_root_module() {
    insta::assert_snapshot!(helpers::run_error("import foo.bar"));
}

// These verify that error spans cover the full postfix expression,
// not just the leftmost atom.

#[test]
fn span_method_call() {
    // Arrow should cover p.nonexistent(), not just p
    insta::assert_snapshot!(helpers::run_error(
        "kind P { x: Int }\nlet p = P { x: 1 }\np.nonexistent()"
    ));
}

#[test]
fn span_chained_attr() {
    // Arrow should cover p.x.y, not just p
    insta::assert_snapshot!(helpers::run_error(
        "kind P { x: Int }\nlet p = P { x: 1 }\np.x.y"
    ));
}

#[test]
fn span_nested_call() {
    // Arrow should cover the full call chain
    insta::assert_snapshot!(helpers::run_error("print(undefined_var)"));
}

#[test]
fn span_type_args() {
    // Arrow should cover Foo[Int], not just Foo
    insta::assert_snapshot!(helpers::run_error("Nonexistent[Int]"));
}

// ── Multi-line and complex errors ───────────────────────────────

#[test]
fn error_on_later_line() {
    insta::assert_snapshot!(helpers::run_error(
        "let a = 1\nlet b = 2\nlet c = 3\nlet d = a + \"oops\""
    ));
}

#[test]
fn error_inside_function() {
    insta::assert_snapshot!(helpers::run_error(
        "func double(x: Int): Int {\n    ret x + \"bad\"\n}\ndouble(1)"
    ));
}

#[test]
fn nested_expr_error() {
    // Error in subexpression — should point to the inner error
    insta::assert_snapshot!(helpers::run_error("1 + 2 * true"));
}

#[test]
fn interp_type_error() {
    // Type error inside string interpolation
    insta::assert_snapshot!(helpers::run_error("print(\"result: {1 + true}\")"));
}

#[test]
fn generic_method_type_error() {
    // Pushing wrong type into a typed array
    insta::assert_snapshot!(helpers::run_error("let a = [1, 2, 3]\na.push(\"wrong\")"));
}

#[test]
fn match_body_error() {
    // Error inside a match arm body
    insta::assert_snapshot!(helpers::run_error(
        "let x = Opt[Int].Val(42)\nmatch x {\n    Val(n) -> n + \"bad\",\n    Non -> 0,\n}"
    ));
}

#[test]
fn deeply_nested_undefined() {
    // Undefined var deep in an expression
    insta::assert_snapshot!(helpers::run_error("print(1 + foo * 2)"));
}
