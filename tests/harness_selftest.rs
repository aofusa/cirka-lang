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

/// The limits of a run (BUG-0041), in a child process (this test binary
/// running only `limits_child`): a case that never ends, and one that
/// takes memory without end, stop the process with their names.
#[test]
fn run_limits_stop_runaway_cases() {
    let exe = std::env::current_exe().unwrap();
    for (mode, key, value) in [
        ("sleep", "CIRKA_TEST_CASE_TIMEOUT", "1"),
        ("grow", "CIRKA_TEST_MAX_RSS_MB", "64"),
    ] {
        let start = std::time::Instant::now();
        let out = std::process::Command::new(&exe)
            .args(["--exact", "limits_child", "--nocapture"])
            .env("CIRKA_SELFTEST_LIMITS", mode)
            .env_remove("CIRKA_TEST_FILTER")
            .env_remove("CIRKA_TEST_CASE_TIMEOUT")
            .env_remove("CIRKA_TEST_MAX_RSS_MB")
            .env(key, value)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{mode}: {err}");
        assert!(
            err.contains("`runaway`") && err.contains(key),
            "{mode}: {err}"
        );
        assert!(start.elapsed().as_secs() < 60, "{mode}: {err}");
    }
}

/// Run by `run_limits_stop_runaway_cases` in a child process: one case
/// that sleeps forever or allocates (and touches) memory forever.
#[test]
fn limits_child() {
    let Ok(mode) = std::env::var("CIRKA_SELFTEST_LIMITS") else {
        return;
    };
    let case = cirka_testkit::Case {
        path: "runaway.txt".into(),
        name: "runaway".into(),
        header: Default::default(),
    };
    let _ = cirka_testkit::run_cases(&[case], |_| {
        let mut held: Vec<Vec<u8>> = Vec::new();
        loop {
            if mode == "grow" {
                held.push(vec![1u8; 8 << 20]);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    unreachable!("the run should have been stopped");
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
