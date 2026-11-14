//! Shared helpers for the integration tests.

use cirka_testkit::{Case, Outcome};

/// Parses `cirka-stats: allocs=N frees=M ...` from stderr.
pub fn parse_stats(stderr: &str) -> Option<(u64, u64)> {
    Some((stat(stderr, "allocs")?, stat(stderr, "frees")?))
}

/// One counter (`allocs`, `frees`, `max_live`, `forced`, `array_copies`) of the
/// `cirka-stats:` line.
pub fn stat(stderr: &str, key: &str) -> Option<u64> {
    let line = stderr.lines().find(|l| l.starts_with("cirka-stats:"))?;
    let k = format!("{key}=");
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(k.as_str()))
        .and_then(|v| v.parse().ok())
}

/// The `-- max-allocs: N`, `-- max-forced: N` and `-- max-array-copies: N`
/// headers (allocations, evaluated thunks and copies of shared arrays by
/// in-place updates of a native run).
fn check_limits(c: &Case, name: &str, stderr: &str) -> Result<(), String> {
    for (h, key) in [
        ("max-allocs", "allocs"),
        ("max-forced", "forced"),
        ("max-array-copies", "array_copies"),
    ] {
        let Some(limit) = c.header.get(h) else {
            continue;
        };
        let limit: u64 = limit
            .trim()
            .parse()
            .map_err(|_| format!("bad `{h}` header"))?;
        match stat(stderr, key) {
            Some(n) if n <= limit => {}
            n => return Err(format!("[{name}] {key} = {n:?}, more than {h}: {limit}")),
        }
    }
    Ok(())
}

/// Builds `m` natively with the debug and release runtimes, runs it and
/// compares stdout, exit code and allocation counts with the interpreter's
/// `reference` result (when `interp_ok`; otherwise the stdout and exit code
/// of `reference`, and as many objects left unfreed as on the debug runtime,
/// whose leak check passed), checking for leaks.
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
    use cirka_driver::native::RuntimeKind;
    let work = cirka_testkit::work_dir(suite, &c.name);
    let mut runs = vec![
        (RuntimeKind::Debug, "debug", None),
        (RuntimeKind::Release, "release", None),
    ];
    // `-- tsan: 1`: also under ThreadSanitizer, with both allocators, where
    // it can run (05-runtime.md §9.7).
    if c.header.get("tsan") == Some("1") && cirka_driver::native::tsan_usable(cfg) {
        if let Err(e) = cirka_driver::native::ensure_tsan_runtime_built() {
            return Outcome::Fail(e);
        }
        runs.push((RuntimeKind::Tsan, "tsan", None));
        runs.push((RuntimeKind::Tsan, "tsan-system", Some("system")));
    }
    let mut native_held: Option<u64> = None;
    for (rt, name, alloc) in runs {
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
        if let Some(a) = alloc {
            cmd.env("CIRKA_ALLOC", a);
        }
        if rt == RuntimeKind::Tsan {
            cmd.env("TSAN_OPTIONS", "halt_on_error=1");
        }
        for (k, v) in c.header.runtime_env() {
            cmd.env(k, v);
        }
        let out = match cirka_testkit::run_with_stdin(cmd, Some(stdin)) {
            Ok(o) => o,
            Err(e) => return Outcome::Fail(e.to_string()),
        };
        if out.stderr.contains("ThreadSanitizer") {
            return Outcome::Fail(format!("[{name}] ThreadSanitizer report:\n{}", out.stderr));
        }
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
        if let Err(e) = check_limits(c, name, &out.stderr) {
            return Outcome::Fail(e);
        }
        if !leak_check {
            continue;
        }
        if out.stderr.contains("objects leaked")
            || out.stderr.contains("debug runtime")
            || out.stderr.contains("leak check skipped")
        {
            return Outcome::Fail(format!("[{name}] runtime report:\n{}", out.stderr));
        }
        match (parse_stats(&out.stderr), native_held) {
            // Without the interpreter, the first run (the debug runtime,
            // whose leak check passed) is the reference: what it did not
            // free is held by CAFs. The other runs must leave as many
            // objects (threads may make the counts themselves vary).
            (Some((a, f)), None) if !interp_ok && rt == RuntimeKind::Debug && a >= f => {
                native_held = Some(a - f);
            }
            (Some((a, f)), Some(held)) if !interp_ok && a >= f && a - f == held => {}
            (Some((a, f)), _)
                if interp_ok && (a, f) == (reference.stats.allocs, reference.stats.frees) => {}
            (s, held) => {
                let want = match held {
                    Some(h) => format!("{h} objects left, as on the debug runtime"),
                    None => format!(
                        "the interpreter's ({}, {})",
                        reference.stats.allocs, reference.stats.frees
                    ),
                };
                return Outcome::Fail(format!("[{name}] allocs/frees {s:?}: expected {want}"));
            }
        }
    }
    Outcome::Pass
}

/// What a program costs at -O2, for the zero-cost tests (FEAT-0061).
#[derive(Clone, Copy, Debug)]
pub struct Cost {
    pub allocs: u64,
    pub rc_ops: u64,
    /// Executed MIR instructions and terminators.
    pub steps: u64,
    /// Instructions and terminators of the optimized MIR.
    pub size: u64,
}

impl Cost {
    pub fn of(m: &cirka_mir::Module, r: &cirka_mir::interp::RunResult) -> Cost {
        Cost {
            allocs: r.stats.allocs,
            rc_ops: r.stats.rc_ops,
            steps: r.stats.steps,
            size: m
                .funcs
                .iter()
                .flat_map(|f| &f.blocks)
                .map(|b| b.instrs.len() as u64 + 1)
                .sum(),
        }
    }
}

/// Compiles `core` at -O2 and returns its cost on `stdin` (the hand-written
/// side of `same-cost-as`).
pub fn cost_at_o2(core: cirka_core::Program, case: &str, stdin: &[u8]) -> Result<Cost, String> {
    let mut opts = cirka_driver::pipeline::Options::new("hand");
    opts.opt_level = 2;
    opts.build_dir = cirka_testkit::work_dir("same-cost", case);
    let out = cirka_driver::pipeline::compile_core(core, "hand", &opts, false).map_err(|ds| {
        ds.iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let r = cirka_mir::interp::run(&out.mir, stdin, u64::MAX)?;
    Ok(Cost::of(&out.mir, &r))
}

/// `-- same-cost-as: FILE [+N]` (FEAT-0061): the program, which uses an
/// abstraction (a newtype, a type class, a higher-order function, a
/// borrow), costs no more at -O2 than FILE, the same program written by
/// hand in the same directory: it allocates no more, executes no more
/// instructions and reference-count operations, and its optimized MIR is
/// no larger (each up to `N`, a constant cost such as entering a borrow).
/// `hand` compiles and runs FILE at -O2.
pub fn check_same_cost(
    c: &Case,
    mine: Cost,
    hand: impl FnOnce(&std::path::Path) -> Result<Cost, String>,
) -> Result<(), String> {
    let Some(h) = c.header.get("same-cost-as") else {
        return Ok(());
    };
    let mut words = h.split_whitespace();
    let file = words.next().ok_or("`same-cost-as` needs a file")?;
    let slack: u64 = match words.next() {
        Some(w) => w
            .strip_prefix('+')
            .and_then(|n| n.parse().ok())
            .ok_or("bad slack in `same-cost-as` (expected `+N`)")?,
        None => 0,
    };
    let path = c.path.parent().unwrap().join(file);
    let other = hand(&path).map_err(|e| format!("same-cost-as {file}: {e}"))?;
    let worse: Vec<&str> = [
        ("allocs", mine.allocs, other.allocs),
        ("rc_ops", mine.rc_ops, other.rc_ops),
        ("steps", mine.steps, other.steps),
        ("size", mine.size, other.size),
    ]
    .into_iter()
    .filter(|(_, a, b)| *a > b + slack)
    .map(|(k, _, _)| k)
    .collect();
    if worse.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "costs more than {file} ({}):\n  this: {mine:?}\n  hand: {other:?}",
            worse.join(", ")
        ))
    }
}
