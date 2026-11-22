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

/// The memcheck wrapper (BUG-0038): a program reading freed memory fails
/// under it, and the same program without that read passes. Skipped where
/// valgrind or the C compiler cannot run.
#[test]
fn valgrind_wrapper_reports_memory_errors() {
    cirka_testkit::init();
    let probe = cirka_testkit::valgrind_command(std::path::Path::new("--version")).output();
    if !probe.is_ok_and(|o| o.status.success()) {
        eprintln!("skipped: valgrind cannot run here");
        return;
    }
    let clang = std::env::var("CIRKA_CLANG").unwrap_or_else(|_| "clang".into());
    let dir = cirka_testkit::work_dir("harness-valgrind", "use_after_free");
    let src = dir.join("uaf.c");
    let exe = dir.join("uaf");
    // Unoptimized, so that the read after `free` stays.
    std::fs::write(
        &src,
        "#include <stdlib.h>\n\
         int main(int argc, char **argv) {\n\
           (void)argv;\n\
           volatile int *p = malloc(4 * sizeof(int));\n\
           p[1] = 7;\n\
           int x = p[1];\n\
           free((void *)p);\n\
           if (argc > 1) x = p[1];\n\
           return x == 7 ? 0 : 0;\n\
         }\n",
    )
    .unwrap();
    let built = std::process::Command::new(&clang)
        .args(["-O0", "-g", "-o"])
        .arg(&exe)
        .arg(&src)
        .status();
    if !built.is_ok_and(|s| s.success()) {
        eprintln!("skipped: {clang} cannot build the probe");
        return;
    }
    let run = |bad: bool| {
        let mut cmd = cirka_testkit::valgrind_command(&exe);
        if bad {
            cmd.arg("read-after-free");
        }
        cirka_testkit::run_with_stdin(cmd, None).unwrap()
    };
    let clean = run(false);
    assert_eq!(clean.code, 0, "{}", clean.stderr);
    assert!(
        cirka_testkit::valgrind_errors(&clean).is_none(),
        "{}",
        clean.stderr
    );
    let bad = run(true);
    assert_eq!(
        bad.code,
        cirka_testkit::VALGRIND_ERROR_CODE,
        "{}",
        bad.stderr
    );
    let report = cirka_testkit::valgrind_errors(&bad).expect("no valgrind report");
    assert!(report.contains("Invalid read"), "{report}");
}
