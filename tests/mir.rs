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
        if c.header.get("skip-backend") == Some("interp") {
            return Outcome::Skip("skip-backend: interp".into());
        }
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

/// Parses `cirka-stats: allocs=N frees=M ...` from stderr.
pub fn parse_stats(stderr: &str) -> Option<(u64, u64)> {
    let line = stderr.lines().find(|l| l.starts_with("cirka-stats:"))?;
    let get = |k: &str| -> Option<u64> {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .and_then(|v| v.parse().ok())
    };
    Some((get("allocs=")?, get("frees=")?))
}

#[test]
fn mir_run_native() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let cases = discover(&repo_root().join("tests/mir/run"), &["mir"]);
    run_suite("mir-run-native", &cases, |c| {
        let m = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let interp_ok = c.header.get("skip-backend") != Some("interp");
        let reference = if interp_ok {
            match cirka_mir::interp::run(&m, &stdin, u64::MAX) {
                Ok(r) => r,
                Err(e) => return Outcome::Fail(format!("interpreter: {e}")),
            }
        } else {
            cirka_mir::interp::RunResult {
                stdout: std::fs::read_to_string(c.sibling("stdout")).unwrap_or_default(),
                stderr: String::new(),
                exit_code: expected_exit(c),
                stats: Default::default(),
                leaked: 0,
            }
        };
        let work = cirka_testkit::work_dir("mir-native", &c.name);
        for (rt, name) in [
            (cirka_driver::native::RuntimeKind::Debug, "debug"),
            (cirka_driver::native::RuntimeKind::Release, "release"),
        ] {
            let exe = work.join(format!("prog-{name}"));
            let opts = cirka_driver::native::NativeOptions {
                runtime: rt,
                ..Default::default()
            };
            let ir = match cirka_driver::native::build_mir(&cfg, &m, &work, &exe, &opts) {
                Ok(ir) => ir,
                Err(e) => return Outcome::Fail(e),
            };
            if let Err(e) = cirka_driver::native::verify_ir(&cfg, &work.join("out.ll")) {
                cirka_testkit::save_failure("mir-native", &c.name, &[("out.ll", &ir)]);
                return Outcome::Fail(format!("opt -passes=verify failed:\n{e}"));
            }
            let mut cmd = std::process::Command::new(&exe);
            cmd.env("CIRKA_STATS", "1").env("CIRKA_LEAK_CHECK", "1");
            for (k, v) in c.header.runtime_env() {
                cmd.env(k, v);
            }
            let out = match cirka_testkit::run_with_stdin(cmd, Some(&stdin)) {
                Ok(o) => o,
                Err(e) => return Outcome::Fail(e.to_string()),
            };
            if out.stdout != reference.stdout {
                cirka_testkit::save_failure("mir-native", &c.name, &[("out.ll", &ir)]);
                return Outcome::Fail(format!(
                    "[{name}] native stdout differs from the interpreter\n{}",
                    cirka_testkit::line_diff(&reference.stdout, &out.stdout)
                ));
            }
            if out.code != reference.exit_code {
                return Outcome::Fail(format!(
                    "[{name}] exit code {} (interpreter: {})\n{}",
                    out.code, reference.exit_code, out.stderr
                ));
            }
            if out.stderr.contains("objects leaked") || out.stderr.contains("debug runtime") {
                return Outcome::Fail(format!("[{name}] runtime report:\n{}", out.stderr));
            }
            match parse_stats(&out.stderr) {
                Some((a, f)) if !interp_ok && a == f => {}
                Some((a, f)) if (a, f) == (reference.stats.allocs, reference.stats.frees) => {}
                s => {
                    return Outcome::Fail(format!(
                        "[{name}] allocs/frees {s:?} differ from the interpreter ({}, {})",
                        reference.stats.allocs, reference.stats.frees
                    ));
                }
            }
        }
        Outcome::Pass
    });
}

/// The hand-written binary-trees benchmark (FEAT-0016) stays valid: it
/// verifies, runs leak-free in the interpreter at a small depth and matches
/// the Rust baseline's output.
#[test]
fn bench_binary_trees_mir() {
    let root = repo_root();
    let src = std::fs::read_to_string(root.join("bench/binary-trees/mir/main.mir")).unwrap();
    let m = cirka_mir::parse(FileId(0), &src).expect("parse");
    let errs = cirka_mir::verify::verify_module(&m);
    assert!(
        errs.is_empty(),
        "{}",
        cirka_mir::verify::render_errors(&errs)
    );
    let r = cirka_mir::interp::run_with_args(&m, &["6"], b"", u64::MAX).unwrap();
    assert_eq!(r.exit_code, 0, "{}", r.stderr);
    assert_eq!(r.leaked, 0);
    let expected = "\
stretch tree of depth 7\t check: 255
64\t trees of depth 4\t check: 1984
16\t trees of depth 6\t check: 2032
long lived tree of depth 6\t check: 127
";
    assert_eq!(r.stdout, expected);
}

/// Runs a module (after Perceus) in the interpreter and checks the case's
/// expected stdout / exit code and that nothing leaks.
fn run_checked(c: &cirka_testkit::Case, m: &cirka_mir::Module) -> Outcome {
    let errs = cirka_mir::verify::verify_module(m);
    if !errs.is_empty() {
        return Outcome::Fail(format!(
            "{}\n{}",
            cirka_mir::verify::render_errors(&errs),
            cirka_mir::print_module(m)
        ));
    }
    let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
    let r = match cirka_mir::interp::run(m, &stdin, u64::MAX) {
        Ok(r) => r,
        Err(e) => return Outcome::Fail(e),
    };
    if let Err(e) = check_expect(&c.sibling("stdout"), &r.stdout) {
        return Outcome::Fail(e);
    }
    if r.exit_code != expected_exit(c) {
        return Outcome::Fail(format!("exit code {} (stderr: {})", r.exit_code, r.stderr));
    }
    if r.leaked != 0 {
        return Outcome::Fail(format!(
            "{} objects leaked\n{}",
            r.leaked,
            cirka_mir::print_module(m)
        ));
    }
    Outcome::Pass
}

/// Perceus snapshots (FEAT-0020): `tests/mir/perceus/*.mir` contain no
/// dup/drop; the annotated output is compared with `.out` and the program
/// must run leak-free.
#[test]
fn mir_perceus() {
    let cases = discover(&repo_root().join("tests/mir/perceus"), &["mir"]);
    assert!(!cases.is_empty());
    run_suite("mir-perceus", &cases, |c| {
        let mut m = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        cirka_mir::perceus::insert_rc(&mut m);
        if let Err(e) = check_expect(&c.sibling("out"), &cirka_mir::print_module(&m)) {
            return Outcome::Fail(e);
        }
        run_checked(c, &m)
    });
}

/// Every hand-annotated run test still works after stripping its dup/drop
/// and re-inserting them with Perceus.
#[test]
fn mir_perceus_roundtrip() {
    let cases = discover(&repo_root().join("tests/mir/run"), &["mir"]);
    run_suite("mir-perceus-roundtrip", &cases, |c| {
        if c.header.get("skip-backend") == Some("interp") || c.header.get("expect-leak").is_some() {
            return Outcome::Skip("not applicable".into());
        }
        let mut m = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        cirka_mir::perceus::strip_rc(&mut m);
        cirka_mir::perceus::insert_rc(&mut m);
        run_checked(c, &m)
    });
}

/// Borrow inference (FEAT-0021): every run test and Perceus snapshot input,
/// with its hand-written ownership removed (all parameters owned, no
/// dup/drop), runs leak-free after borrow inference + Perceus and executes
/// no more dup/drop operations than with Perceus alone. The counts are
/// written to `_build/borrow-inference.txt`.
#[test]
fn mir_borrow_inference() {
    let root = repo_root();
    let mut cases = discover(&root.join("tests/mir/run"), &["mir"]);
    cases.extend(discover(&root.join("tests/mir/perceus"), &["mir"]));
    let report = std::sync::Mutex::new(Vec::new());
    run_suite("mir-borrow", &cases, |c| {
        if c.header.get("skip-backend") == Some("interp") || c.header.get("expect-leak").is_some() {
            return Outcome::Skip("not applicable".into());
        }
        let mut base = match cirka_mir::parse(FileId(0), &c.source()) {
            Ok(m) => m,
            Err(e) => return Outcome::Fail(format!("parse error: {}", e.message)),
        };
        cirka_mir::perceus::strip_rc(&mut base);
        for f in &mut base.funcs {
            for p in &mut f.params {
                p.own = cirka_mir::Own::Owned;
            }
        }
        let mut plain = base.clone();
        cirka_mir::perceus::insert_rc(&mut plain);
        let mut inferred = base;
        cirka_mir::perceus::run(&mut inferred);
        if let Outcome::Fail(e) = run_checked(c, &inferred) {
            return Outcome::Fail(e);
        }
        if let Outcome::Fail(e) = run_checked(c, &plain) {
            // In-place primitives (array_set) need a unique object; with
            // every parameter owned the caller must dup it, so only the
            // inferred version can run.
            if e.contains("on a shared object") {
                report
                    .lock()
                    .unwrap()
                    .push(format!("{}: (needs borrowing) -> ok", c.name));
                return Outcome::Pass;
            }
            return Outcome::Fail(e);
        }
        let ops = |m: &cirka_mir::Module| {
            cirka_mir::interp::run(m, &[], u64::MAX)
                .map(|r| r.stats.rc_ops)
                .unwrap_or(0)
        };
        let (a, b) = (ops(&plain), ops(&inferred));
        report
            .lock()
            .unwrap()
            .push(format!("{}: {a} -> {b}", c.name));
        if b > a {
            return Outcome::Fail(format!(
                "borrow inference increased dup/drop count: {a} -> {b}"
            ));
        }
        Outcome::Pass
    });
    let mut lines = report.into_inner().unwrap();
    lines.sort();
    let dir = root.join("_build");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("borrow-inference.txt"), lines.join("\n") + "\n");
}
