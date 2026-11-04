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
