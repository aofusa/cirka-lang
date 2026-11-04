//! Self-test of the shared harness (FEAT-0003): a trivial "compiler" that
//! echoes the file (upper-cased when `-- flags: --upper` is given).

use cirka_testkit::{Outcome, check_expect, discover, repo_root, run_suite};

#[test]
fn selftest_suite() {
    let dir = repo_root().join("tests/harness/selftest");
    let cases = discover(&dir, &["txt"]);
    assert_eq!(cases.len(), 2);
    run_suite("harness-selftest", &cases, |c| {
        let src = c.source();
        let out = if c.header.flags().iter().any(|f| f == "--upper") {
            src.to_uppercase()
        } else {
            src
        };
        match check_expect(&c.sibling("stdout"), &out) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}
