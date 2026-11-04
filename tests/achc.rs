//! aCHC front end tests: parse snapshots and syntax errors (FEAT-0100,
//! FEAT-0101).

use cirka_diagnostics::{SourceMap, Style, render};
use cirka_testkit::{Outcome, check_expect, discover, repo_root, run_suite};

fn render_all(name: &str, src: &str, errs: &[cirka_diagnostics::Diagnostic]) -> String {
    let mut sm = SourceMap::new();
    sm.add(name, src);
    errs.iter().map(|d| render(d, &sm, Style::Plain)).collect()
}

/// tests/achc/parse/*.hs parse without errors; the AST dump matches `.ast`.
#[test]
fn achc_parse_snapshots() {
    let cases = discover(&repo_root().join("tests/achc/parse"), &["hs", "chs"]);
    assert!(!cases.is_empty());
    run_suite("achc-parse", &cases, |c| {
        let src = c.source();
        let (m, errs) = achc_syntax::parse(cirka_diagnostics::FileId(0), &src);
        if !errs.is_empty() {
            return Outcome::Fail(render_all(&c.name, &src, &errs));
        }
        match check_expect(&c.sibling("ast"), &achc_syntax::print::print_module(&m)) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// tests/achc/fail/syntax/*.hs report the errors in `.stderr`.
#[test]
fn achc_syntax_errors() {
    let cases = discover(&repo_root().join("tests/achc/fail/syntax"), &["hs"]);
    assert!(!cases.is_empty());
    run_suite("achc-syntax-fail", &cases, |c| {
        let src = c.source();
        let (_, errs) = achc_syntax::parse(cirka_diagnostics::FileId(0), &src);
        if errs.is_empty() {
            return Outcome::Fail("no syntax error reported".into());
        }
        match check_expect(
            &c.sibling("stderr"),
            &render_all(&format!("{}.hs", c.name), &src, &errs),
        ) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}
