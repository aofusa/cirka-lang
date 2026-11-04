//! Shared helpers for the integration tests.

use cirka_testkit::{Case, Outcome};

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

/// Builds `m` natively with the debug and release runtimes, runs it and
/// compares stdout, exit code and allocation counts with the interpreter's
/// `reference` result (when `interp_ok`), checking for leaks.
pub fn check_native(
    cfg: &cirka_driver::Config,
    c: &Case,
    suite: &str,
    m: &cirka_mir::Module,
    stdin: &[u8],
    reference: &cirka_mir::interp::RunResult,
    interp_ok: bool,
) -> Outcome {
    check_native_with(cfg, c, suite, m, stdin, reference, interp_ok, true)
}

/// [`check_native`]; with `leak_check` off (a program ending by `panic` or
/// `exit`, which do not unwind) live objects and allocation counts are not
/// checked.
#[allow(clippy::too_many_arguments)]
pub fn check_native_with(
    cfg: &cirka_driver::Config,
    c: &Case,
    suite: &str,
    m: &cirka_mir::Module,
    stdin: &[u8],
    reference: &cirka_mir::interp::RunResult,
    interp_ok: bool,
    leak_check: bool,
) -> Outcome {
    let work = cirka_testkit::work_dir(suite, &c.name);
    for (rt, name) in [
        (cirka_driver::native::RuntimeKind::Debug, "debug"),
        (cirka_driver::native::RuntimeKind::Release, "release"),
    ] {
        let exe = work.join(format!("prog-{name}"));
        let opts = cirka_driver::native::NativeOptions {
            runtime: rt,
            ..Default::default()
        };
        let ir = match cirka_driver::native::build_mir(cfg, m, &work, &exe, &opts) {
            Ok(ir) => ir,
            Err(e) => return Outcome::Fail(e),
        };
        if let Err(e) = cirka_driver::native::verify_ir(cfg, &work.join("out.ll")) {
            cirka_testkit::save_failure(suite, &c.name, &[("out.ll", &ir)]);
            return Outcome::Fail(format!("opt -passes=verify failed:\n{e}"));
        }
        let mut cmd = std::process::Command::new(&exe);
        cmd.env("CIRKA_STATS", "1").env("CIRKA_LEAK_CHECK", "1");
        for (k, v) in c.header.runtime_env() {
            cmd.env(k, v);
        }
        let out = match cirka_testkit::run_with_stdin(cmd, Some(stdin)) {
            Ok(o) => o,
            Err(e) => return Outcome::Fail(e.to_string()),
        };
        if out.stdout != reference.stdout {
            cirka_testkit::save_failure(suite, &c.name, &[("out.ll", &ir)]);
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
        if !leak_check {
            continue;
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
}
