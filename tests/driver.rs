//! Shared driver tests (FEAT-0032): both CLIs (`achc` and `cirka`) compile
//! Core input through the same pipeline and give the same results.

use cirka_testkit::{Outcome, discover, repo_root, run_suite};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// Builds both CLIs once and returns their paths.
fn bins() -> &'static (PathBuf, PathBuf) {
    static BINS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    BINS.get_or_init(|| {
        let st = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "achc", "-p", "cirka"])
            .current_dir(repo_root())
            .status()
            .unwrap();
        assert!(st.success(), "building achc/cirka failed");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root().join("target"));
        let target = if target.is_relative() {
            repo_root().join(target)
        } else {
            target
        };
        let dir = target.join("debug");
        (dir.join("achc"), dir.join("cirka"))
    })
}

fn run(bin: &PathBuf, args: &[&str], cwd: &std::path::Path) -> cirka_testkit::RunOutput {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd).env("CIRKA_LEAK_CHECK", "1");
    cirka_testkit::run_with_stdin(cmd, None).unwrap()
}

/// `achc run` and `cirka run` on every tests/core/run program print the
/// expected output with the same exit code, at -O0 and -O2.
#[test]
fn both_clis_agree_on_core_input() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (achc, cirka) = bins();
    let cases = discover(&repo_root().join("tests/core/run"), &["core"]);
    assert!(!cases.is_empty());
    run_suite("driver-core", &cases, |c| {
        let expected = std::fs::read_to_string(c.sibling("stdout")).unwrap_or_default();
        // Without `.exitcode`, `main`'s result is the exit code: all runs
        // must agree with each other.
        let mut code: Option<i32> = std::fs::read_to_string(c.sibling("exitcode"))
            .ok()
            .and_then(|s| s.trim().parse().ok());
        let src = c.path.to_str().unwrap();
        let mut outs = Vec::new();
        for (tool, bin) in [("achc", achc), ("cirka", cirka)] {
            for (o, rt) in [("-O0", "--runtime=debug"), ("-O2", "--runtime=release")] {
                let work = cirka_testkit::work_dir("driver-core", &format!("{}-{tool}{o}", c.name));
                let bd = format!("--build-dir={}", work.display());
                let out = run(bin, &["run", o, rt, &bd, src], &work);
                let want = *code.get_or_insert(out.code);
                if out.stdout != expected || out.code != want {
                    return Outcome::Fail(format!(
                        "{tool} {o}: exit {} (expected {want})\n{}\n{}",
                        out.code,
                        cirka_testkit::line_diff(&expected, &out.stdout),
                        out.stderr
                    ));
                }
                if out.stderr.contains("leaked") {
                    return Outcome::Fail(format!("{tool} {o}: {}", out.stderr));
                }
                outs.push(out);
            }
        }
        if outs.windows(2).any(|w| w[0].stdout != w[1].stdout) {
            return Outcome::Fail("CLIs disagree".into());
        }
        Outcome::Pass
    });
}

/// `check` reports Lint errors of a `.core` file as diagnostics (exit 1)
/// identically from both CLIs, and `--dump` writes every stage.
#[test]
fn check_and_dump() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (achc, cirka) = bins();
    let fail = discover(&repo_root().join("tests/core/fail"), &["core"]);
    let bad = fail[0].path.to_str().unwrap().to_string();
    let work = cirka_testkit::work_dir("driver-misc", "check");
    let a = run(achc, &["check", &bad], &work);
    let b = run(cirka, &["check", &bad], &work);
    assert_eq!(a.code, 1, "{}", a.stderr);
    assert!(a.stderr.contains("error"), "{}", a.stderr);
    assert_eq!(a, b);

    let ok = repo_root().join("tests/core/run/haskell_style.core");
    let bd = work.join("dump");
    let out = run(
        cirka,
        &[
            "build",
            "--dump=all",
            &format!("--build-dir={}", bd.display()),
            "-o",
            work.join("hs").to_str().unwrap(),
            ok.to_str().unwrap(),
        ],
        &work,
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    for ext in ["core", "opt.core", "mir", "opt.mir", "ll"] {
        let p = bd.join(format!("haskell_style.{ext}"));
        assert!(p.exists(), "missing dump {}", p.display());
    }
    let r = run(&work.join("hs"), &[], &work);
    assert_eq!(
        r.stdout,
        std::fs::read_to_string(ok.with_extension("stdout")).unwrap()
    );

    let u = run(achc, &["frobnicate", "x.core"], &work);
    assert_eq!(u.code, 2);
    assert!(u.stderr.contains("unknown command"));
}

/// `cirka test` on tests/cirkalang/test/*.cirka (FEAT-0216): each test runs
/// in its own process; the report is `.stdout` and the exit code
/// `.exitcode` (default 0). `--filter=` selects tests by name.
#[test]
fn cirka_source_tests() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (achc, cirka) = bins();
    let cases = discover(&repo_root().join("tests/cirkalang/test"), &["cirka"]);
    assert!(!cases.is_empty());
    run_suite("cirka-test", &cases, |c| {
        let work = cirka_testkit::work_dir("cirka-test", &c.name);
        let bd = format!("--build-dir={}", work.display());
        let dir = c.path.parent().unwrap();
        let file = c.path.file_name().unwrap().to_str().unwrap();
        let out = run(cirka, &["test", &bd, file], dir);
        let want: i32 = std::fs::read_to_string(c.sibling("exitcode"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap_or(0);
        if out.code != want {
            return Outcome::Fail(format!(
                "exit {} (expected {want})\n{}{}",
                out.code, out.stdout, out.stderr
            ));
        }
        if let Err(e) = cirka_testkit::check_expect(&c.sibling("stdout"), &out.stdout) {
            return Outcome::Fail(e);
        }
        // Filtering, and the same file still builds and runs as a program.
        let f = run(cirka, &["test", &bd, "--filter=twice", file], dir);
        if !f.stdout.contains("filtered out") || f.code != 0 && want == 0 {
            return Outcome::Fail(format!("--filter: {}{}", f.stdout, f.stderr));
        }
        Outcome::Pass
    });
    // Haskell sources have no tests in source files.
    let hs = repo_root().join("tests/achc/run/h1/fact.hs");
    let work = cirka_testkit::work_dir("cirka-test", "achc");
    let bd = format!("--build-dir={}", work.display());
    let out = run(achc, &["test", &bd, hs.to_str().unwrap()], &work);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("not supported"), "{}", out.stderr);
}
