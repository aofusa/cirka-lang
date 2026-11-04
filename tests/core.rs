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

/// Core → MIR → Perceus for every program in tests/core/run: the MIR
/// verifies, the interpreter prints `.stdout` without leaks, and native code
/// (debug and release runtimes) agrees with the interpreter (FEAT-0031).
#[test]
fn core_run() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let cases = discover(&repo_root().join("tests/core/run"), &["core"]);
    run_suite("core-run", &cases, |c| {
        let p = match parse(c) {
            Ok(p) => p,
            Err(e) => return Outcome::Fail(e),
        };
        let mut m = match cirka_mir::lower::lower_program(&p) {
            Ok(m) => m,
            Err(errs) => return Outcome::Fail(cirka_core::lint::render_errors(&errs)),
        };
        cirka_mir::perceus::run(&mut m);
        let errs = cirka_mir::verify::verify_module(&m);
        if !errs.is_empty() {
            return Outcome::Fail(format!(
                "{}\n{}",
                cirka_mir::verify::render_errors(&errs),
                cirka_mir::print_module(&m)
            ));
        }
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let r = match cirka_mir::interp::run(&m, &stdin, u64::MAX) {
            Ok(r) => r,
            Err(e) => return Outcome::Fail(format!("{e}\n{}", cirka_mir::print_module(&m))),
        };
        if let Err(e) = check_expect(&c.sibling("stdout"), &r.stdout) {
            return Outcome::Fail(e);
        }
        if r.leaked != 0 {
            return Outcome::Fail(format!(
                "{} objects leaked\n{}",
                r.leaked,
                cirka_mir::print_module(&m)
            ));
        }
        cirka_integration::check_native(&cfg, c, "core-native", &m, &stdin, &r, true)
    });
}

/// Interprets `p` after Core→MIR and Perceus.
fn interp_core(
    p: &cirka_core::Program,
    stdin: &[u8],
) -> Result<cirka_mir::interp::RunResult, String> {
    let mut m =
        cirka_mir::lower::lower_program(p).map_err(|e| cirka_core::lint::render_errors(&e))?;
    cirka_mir::perceus::run(&mut m);
    let errs = cirka_mir::verify::verify_module(&m);
    if !errs.is_empty() {
        return Err(cirka_mir::verify::render_errors(&errs));
    }
    cirka_mir::interp::run(&m, stdin, u64::MAX).map_err(|e| e.to_string())
}

/// The Core simplifier (FEAT-0033) keeps the results of every program in
/// tests/core/run (stdout, exit code, no leaks, Lint after each pass) and
/// never increases the number of allocations; `opt_inline` must get
/// strictly fewer. The counts are written to `_build/core-opt-allocs.txt`.
#[test]
fn core_opt_reduces_allocations() {
    let cases = discover(&repo_root().join("tests/core/run"), &["core"]);
    let table = std::sync::Mutex::new(Vec::new());
    run_suite("core-opt", &cases, |c| {
        let p = match parse(c) {
            Ok(p) => p,
            Err(e) => return Outcome::Fail(e),
        };
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let before = match interp_core(&p, &stdin) {
            Ok(r) => r,
            Err(e) => return Outcome::Fail(e),
        };
        let mut q = p.clone();
        let st = match cirka_core::opt::optimize_with_stats(&mut q) {
            Ok(st) => st,
            Err(e) => return Outcome::Fail(e.message),
        };
        let after = match interp_core(&q, &stdin) {
            Ok(r) => r,
            Err(e) => return Outcome::Fail(format!("{e}\n{}", cirka_core::print_program(&q))),
        };
        if (&after.stdout, after.exit_code) != (&before.stdout, before.exit_code) {
            return Outcome::Fail(format!(
                "optimized program differs (exit {} vs {})\n{}",
                after.exit_code,
                before.exit_code,
                cirka_testkit::line_diff(&before.stdout, &after.stdout)
            ));
        }
        if after.leaked != 0 {
            return Outcome::Fail(format!(
                "{} objects leaked after optimization",
                after.leaked
            ));
        }
        let (a0, a1) = (before.stats.allocs, after.stats.allocs);
        table.lock().unwrap().push(format!(
            "{:<16} allocs {a0:>8} -> {a1:>8}  ({st:?})",
            c.name
        ));
        if a1 > a0 || (c.name == "opt_inline" && a1 >= a0) {
            return Outcome::Fail(format!("allocations {a0} -> {a1}"));
        }
        Outcome::Pass
    });
    let mut t = table.into_inner().unwrap();
    t.sort();
    let out = repo_root().join("_build/core-opt-allocs.txt");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::fs::write(out, t.join("\n") + "\n").unwrap();
}
