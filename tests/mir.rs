//! MIR tests: verifier failures (`tests/mir/fail`) and execution
//! (`tests/mir/*.mir`) on the interpreter and natively.

use cirka_diagnostics::FileId;
use cirka_testkit::{Outcome, check_expect, discover, repo_root, run_suite};

#[test]
fn mir_verifier_failures() {
    let cases = discover(&repo_root().join("tests/mir/fail"), &["mir"]);
    assert!(!cases.is_empty());
    run_suite("mir-fail", &cases, |c| {
        let m = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        let errs = cirka_mir::verify::verify_module(&m);
        if errs.is_empty() {
            return Outcome::Fail("verifier accepted an invalid program".into());
        }
        match check_expect(
            &c.sibling("stderr"),
            &cirka_mir::verify::render_errors(&errs),
        ) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// Expected exit code from `<case>.exitcode` (default 0).
pub fn expected_exit(c: &cirka_testkit::Case) -> i32 {
    std::fs::read_to_string(c.sibling("exitcode"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[test]
fn mir_run_interpreter() {
    let cases = discover(&repo_root().join("tests/mir/run"), &["mir"]);
    assert!(!cases.is_empty());
    run_suite("mir-run-interp", &cases, |c| {
        let m = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        let errs = cirka_mir::verify::verify_module(&m);
        if !errs.is_empty() {
            return Outcome::Fail(cirka_mir::verify::render_errors(&errs));
        }
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let r = match cirka_mir::interp::run(&m, &stdin, u64::MAX) {
            Ok(r) => r,
            Err(e) => return Outcome::Fail(e),
        };
        if let Err(e) = check_expect(&c.sibling("stdout"), &r.stdout) {
            return Outcome::Fail(e);
        }
        if r.exit_code != expected_exit(c) {
            return Outcome::Fail(format!(
                "exit code {} (expected {})\nstderr: {}",
                r.exit_code,
                expected_exit(c),
                r.stderr
            ));
        }
        let expect_leak: u64 = c
            .header
            .get("expect-leak")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if r.leaked != expect_leak {
            return Outcome::Fail(format!(
                "{} objects leaked (expected {expect_leak})",
                r.leaked
            ));
        }
        Outcome::Pass
    });
}
