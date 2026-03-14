// Conformance test runner.
//
// Each test invokes the `kata` binary as a subprocess and compares stdout/stderr
// against fixture files in `tests/ks/<category>/`:
//   <name>.ks           — the input program
//   <name>.expected     — expected stdout (exit 0 asserted)
//   <name>.expected_err — expected stderr fragment (nonzero exit asserted)
//
// To add a new test, add a `.ks` + `.expected` (or `.expected_err`) pair under
// `tests/ks/` and register a one-liner here in the matching `mod` block.

use std::path::Path;
use std::process::Command;

fn run_conformance_test(ks_path: &Path) {
    let bin = env!("CARGO_BIN_EXE_kata");

    let expected_path = ks_path.with_extension("expected");
    let expected_err_path = ks_path.with_extension("expected_err");

    let output = Command::new(bin)
        .args(["ks", ks_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {bin}: {e}"));

    if expected_path.exists() {
        let expected = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", expected_path.display()));
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert!(
            output.status.success(),
            "{}: expected exit 0, got {}\nstderr: {}",
            ks_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            stdout.as_ref(),
            expected.as_str(),
            "{}: stdout mismatch",
            ks_path.display()
        );
    } else if expected_err_path.exists() {
        let fragment = std::fs::read_to_string(&expected_err_path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", expected_err_path.display()));
        let fragment = fragment.trim();
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !output.status.success(),
            "{}: expected nonzero exit, got 0\nstdout: {}",
            ks_path.display(),
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains(fragment),
            "{}: expected stderr to contain {:?}\ngot: {}",
            ks_path.display(),
            fragment,
            stderr
        );
    } else {
        panic!(
            "{}: no .expected or .expected_err fixture found",
            ks_path.display()
        );
    }
}

// ── Test registration ─────────────────────────────────────────────────────────
//
// Each mod maps to a subdirectory under tests/ks/.
// Path is relative to the workspace root (where Cargo.toml lives).

mod print {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/ks/print")
            .join(name)
    }

    #[test]
    fn hello() {
        run_conformance_test(&fixture("hello.ks"));
    }
}
