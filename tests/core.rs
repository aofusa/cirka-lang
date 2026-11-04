//! Common Core tests (FEAT-0030): Lint of valid and invalid programs, and
//! printer/parser round trips.

use cirka_diagnostics::FileId;
use cirka_testkit::{Outcome, check_expect, discover, repo_root, run_suite};

fn parse(c: &cirka_testkit::Case) -> Result<cirka_core::Program, String> {
    cirka_core::parse_program(FileId(0), &c.source()).map_err(|d| {
        let mut sm = cirka_diagnostics::SourceMap::new();
        sm.add(c.name.clone(), c.source());
        cirka_diagnostics::render(&d, &sm, cirka_diagnostics::Style::Plain)
    })
}

/// Every program under tests/core/run passes Lint and survives
/// print → parse → print unchanged.
#[test]
fn core_lint_ok_and_roundtrip() {
    let cases = discover(&repo_root().join("tests/core/run"), &["core"]);
    assert!(!cases.is_empty());
    run_suite("core-lint-ok", &cases, |c| {
        let p = match parse(c) {
            Ok(p) => p,
            Err(e) => return Outcome::Fail(e),
        };
        let errs = cirka_core::lint::lint_program(&p);
        if !errs.is_empty() {
            return Outcome::Fail(cirka_core::lint::render_errors(&errs));
        }
        let text = cirka_core::print_program(&p);
        let p2 = match cirka_core::parse_program(FileId(0), &text) {
            Ok(p) => p,
            Err(e) => return Outcome::Fail(format!("reparse: {}\n{text}", e.message)),
        };
        let errs = cirka_core::lint::lint_program(&p2);
        if !errs.is_empty() {
            return Outcome::Fail(format!(
                "{}\n{text}",
                cirka_core::lint::render_errors(&errs)
            ));
        }
        let text2 = cirka_core::print_program(&p2);
        if text != text2 {
            return Outcome::Fail(cirka_testkit::line_diff(&text, &text2));
        }
        Outcome::Pass
    });
}

/// Every program under tests/core/fail is rejected by Lint with the
/// messages in its `.stderr`.
#[test]
fn core_lint_failures() {
    let cases = discover(&repo_root().join("tests/core/fail"), &["core"]);
    assert!(!cases.is_empty());
    run_suite("core-lint-fail", &cases, |c| {
        let out = match parse(c) {
            Ok(p) => {
                let errs = cirka_core::lint::lint_program(&p);
                if errs.is_empty() {
                    return Outcome::Fail("Lint accepted the program".into());
                }
                cirka_core::lint::render_errors(&errs)
            }
            Err(e) => e,
        };
        match check_expect(&c.sibling("stderr"), &out) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}
