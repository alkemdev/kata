// Conformance test runner.
//
// Auto-discovers every `.ks` file under `tests/ks/` and runs it as a test.
// Adding a test is purely a file operation — no Rust edits required:
//
//   tests/ks/<category>/<name>.ks           input program
//   tests/ks/<category>/<name>.expected     expected stdout  (asserts exit 0)
//   tests/ks/<category>/<name>.expected_err expected stderr fragment (asserts nonzero exit)
//
// Test names are derived from the file path relative to the root, e.g.:
//   run/print/hello.ks   →  filterable as `cargo test --test conformance print/`

use std::path::Path;
use std::process::Command;

fn run(path: &Path) -> datatest_stable::Result<()> {
    let bin = env!("CARGO_BIN_EXE_kata");

    let expected_path = path.with_extension("expected");
    let expected_err_path = path.with_extension("expected_err");

    let output = Command::new(bin)
        .args(["ks", path.to_str().unwrap()])
        .output()
        .map_err(|e| format!("failed to spawn {bin}: {e}"))?;

    if expected_path.exists() {
        let expected = std::fs::read_to_string(&expected_path)
            .map_err(|e| format!("could not read {}: {e}", expected_path.display()))?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        if !output.status.success() {
            return Err(format!(
                "expected exit 0, got {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        if stdout.as_ref() != expected.as_str() {
            return Err(
                format!("stdout mismatch\nexpected: {expected:?}\ngot:      {stdout:?}").into(),
            );
        }
    } else if expected_err_path.exists() {
        let fragment = std::fs::read_to_string(&expected_err_path)
            .map_err(|e| format!("could not read {}: {e}", expected_err_path.display()))?;
        let fragment = fragment.trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            return Err(format!(
                "expected nonzero exit, got 0\nstdout: {}",
                String::from_utf8_lossy(&output.stdout)
            )
            .into());
        }
        if !stderr.contains(fragment.as_str()) {
            return Err(format!("expected stderr to contain {fragment:?}\ngot: {stderr}").into());
        }
    } else {
        return Err(format!(
            "no .expected or .expected_err fixture found for {}",
            path.display()
        )
        .into());
    }

    Ok(())
}

datatest_stable::harness!(run, "../tests/ks", r"\.ks$");
