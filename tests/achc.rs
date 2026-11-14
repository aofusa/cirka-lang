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

/// Multi-module programs under tests/achc/modules/<name>/Main.hs
/// (FEAT-0102): the renamed main module matches `<name>.rn`, or the
/// diagnostics match `<name>.stderr` when renaming fails.
#[test]
fn achc_modules() {
    let dir = repo_root().join("tests/achc/modules");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(!names.is_empty());
    let root = format!("{}/", repo_root().display());
    let mut failed = Vec::new();
    for n in names {
        let mdir = dir.join(&n);
        let main = mdir.join("Main.hs");
        let src = std::fs::read_to_string(&main).unwrap();
        let mut sm = SourceMap::new();
        let id = sm.add(main.display().to_string(), src.clone());
        let mut loader = achc_rename::FsLoader { roots: vec![mdir] };
        let opts = achc_rename::Options::default();
        let r = achc_rename::rename_program(&mut sm, id, &src, false, &mut loader, &opts);
        let (path, text) = match r {
            Ok(p) => (
                dir.join(format!("{n}.rn")),
                achc_syntax::print::print_module(&p.modules.last().unwrap().ast),
            ),
            Err(errs) => (
                dir.join(format!("{n}.stderr")),
                errs.iter()
                    .map(|d| render(d, &sm, Style::Plain))
                    .collect::<String>()
                    .replace(&root, ""),
            ),
        };
        if let Err(e) = check_expect(&path, &text) {
            failed.push(format!("{n}: {e}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Type checker tests (FEAT-0103) against the small library in
/// tests/achc/typeck/lib: `ok/*.hs` (and `.chs`) type check, the inferred
/// bindings of `Main` match `.tc` and the warnings `.warn` (none when
/// absent); `fail/*.hs` report the errors in `.stderr`. The ownership level
/// is `strict` unless the header says `-- flags: --ownership=…`.
#[test]
fn achc_typeck() {
    let dir = repo_root().join("tests/achc/typeck");
    let root = format!("{}/", repo_root().display());
    let mut failed = Vec::new();
    for (sub, ok) in [("ok", true), ("fail", false)] {
        let cases = discover(&dir.join(sub), &["hs", "chs"]);
        assert!(!cases.is_empty());
        for c in cases {
            let src = c.source();
            let mut sm = SourceMap::new();
            let id = sm.add(c.path.display().to_string(), src.clone());
            let mut loader = achc_rename::FsLoader {
                roots: vec![dir.join("lib"), c.path.parent().unwrap().to_path_buf()],
            };
            let opts = achc_rename::Options::default();
            let chs = c.path.extension().is_some_and(|e| e == "chs");
            // `-- flags: --ownership=…` (default strict here).
            let mut dopts = cirka_driver::pipeline::Options::new(&c.name);
            dopts.frontend_flags = vec!["--ownership=strict".into()];
            dopts.frontend_flags.extend(c.header.flags());
            let level = achc::ownership_level(&dopts).unwrap();
            let r = achc_rename::rename_program(&mut sm, id, &src, chs, &mut loader, &opts)
                .and_then(|p| achc_typeck::typecheck_in(&p, None, &|_| false, level));
            let (ext, text) = match r {
                Ok(p) => {
                    let warn: String = p
                        .warnings
                        .iter()
                        .map(|d| render(d, &sm, Style::Plain))
                        .collect::<String>()
                        .replace(&root, "");
                    let wpath = c.sibling("warn");
                    if (!warn.is_empty() || wpath.exists())
                        && let Err(e) = check_expect(&wpath, &warn)
                    {
                        failed.push(format!("{sub}/{}: warnings: {e}", c.name));
                    }
                    ("tc", achc_typeck::tast::print_module_binds(&p, "Main"))
                }
                Err(errs) => (
                    "stderr",
                    errs.iter()
                        .map(|d| render(d, &sm, Style::Plain))
                        .collect::<String>()
                        .replace(&root, ""),
                ),
            };
            if (ext == "tc") != ok {
                failed.push(format!("{sub}/{}: unexpected result\n{text}", c.name));
                continue;
            }
            if let Err(e) = check_expect(&c.sibling(ext), &text) {
                failed.push(format!("{sub}/{}: {e}", c.name));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Desugaring to Core (FEAT-0104): the Core of the `Main` bindings
/// matches `.core` and the warnings match `.warn`.
#[test]
fn achc_desugar() {
    let dir = repo_root().join("tests/achc/desugar");
    let root = format!("{}/", repo_root().display());
    let cases = discover(&dir, &["hs"]);
    assert!(!cases.is_empty());
    let mut failed = Vec::new();
    for c in cases {
        let src = c.source();
        let mut sm = SourceMap::new();
        let id = sm.add(c.path.display().to_string(), src.clone());
        let mut loader = achc_rename::FsLoader {
            roots: vec![repo_root().join("tests/achc/typeck/lib")],
        };
        let opts = achc_rename::Options::default();
        let r = achc_rename::rename_program(&mut sm, id, &src, false, &mut loader, &opts)
            .and_then(|p| achc_typeck::typecheck(&p))
            .and_then(|t| achc_desugar::desugar(&t));
        let out = match r {
            Ok(o) => o,
            Err(errs) => {
                let text: String = errs
                    .iter()
                    .map(|d| render(d, &sm, Style::Plain))
                    .collect::<String>()
                    .replace(&root, "");
                // A `.stderr` file expects the program to be rejected.
                let epath = c.sibling("stderr");
                if epath.exists() {
                    if let Err(e) = check_expect(&epath, &text) {
                        failed.push(format!("{}: {e}", c.name));
                    }
                } else {
                    failed.push(format!("{}: {text}", c.name));
                }
                continue;
            }
        };
        let core: String = out
            .program
            .binds
            .iter()
            .filter(|b| b.name.starts_with("Main."))
            .map(cirka_core::print::print_bind)
            .collect::<Vec<_>>()
            .join("\n");
        let warn: String = out
            .warnings
            .iter()
            .map(|d| render(d, &sm, Style::Plain))
            .collect::<String>()
            .replace(&root, "");
        for (ext, text) in [("core", core), ("warn", warn)] {
            if let Err(e) = check_expect(&c.sibling(ext), &text) {
                failed.push(format!("{}: {e}", c.name));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// MIR snapshots under tests/achc/mir/ (FEAT-0121): the MIR of the `Main`
/// functions after Perceus at -O0 matches `.mir`. A `-- no-rc: f g` header
/// line asserts that those functions (and the thunks and lambdas lifted
/// out of them) contain no `dup` / `drop`.
#[test]
fn achc_mir() {
    let dir = repo_root().join("tests/achc/mir");
    let root = format!("{}/", repo_root().display());
    let mut failed = Vec::new();
    for c in discover(&dir, &["hs"]) {
        let mut sm = SourceMap::new();
        let opts = cirka_driver::pipeline::Options::new(&c.name);
        let roots = vec![dir.clone(), achc::lib_dir()];
        let mut core = match achc::compile(&mut sm, &c.path, &roots, &opts, "test") {
            Ok(x) => x.core,
            Err(ds) => {
                let text: String = ds.iter().map(|d| render(d, &sm, Style::Plain)).collect();
                failed.push(format!("{}: {}", c.name, text.replace(&root, "")));
                continue;
            }
        };
        cirka_core::opt::dce_globals(&mut core);
        let mut m = match cirka_mir::lower::lower_program(&core) {
            Ok(m) => m,
            Err(e) => {
                failed.push(format!("{}: lowering failed: {e:?}", c.name));
                continue;
            }
        };
        cirka_mir::perceus::run(&mut m);
        let funcs: Vec<_> = m
            .funcs
            .iter()
            .filter(|f| f.name.starts_with("Main."))
            .collect();
        let text: String = funcs
            .iter()
            .map(|f| cirka_mir::print::print_function(&m, f) + "\n")
            .collect();
        let src = c.source();
        let no_rc: Vec<&str> = src
            .lines()
            .filter_map(|l| l.strip_prefix("-- no-rc:"))
            .flat_map(|l| l.split_whitespace())
            .collect();
        for name in no_rc {
            let main = format!("Main.{name}");
            for f in funcs
                .iter()
                .filter(|f| f.name == main || f.name.starts_with(&format!("{main}$")))
            {
                let body = cirka_mir::print::print_function(&m, f);
                if body.lines().any(|l| {
                    let l = l.trim_start();
                    l.starts_with("dup ") || l.starts_with("drop ")
                }) {
                    failed.push(format!(
                        "{}: `{}` has RC operations:\n{body}",
                        c.name, f.name
                    ));
                }
            }
        }
        if let Err(e) = check_expect(&c.sibling("mir"), &text) {
            failed.push(format!("{}: {e}", c.name));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Haskell programs under tests/achc/run/<milestone>/ (FEAT-0105): the
/// interpreter prints `.stdout` (exit code `.exitcode`, default 0) at -O0
/// and -O2 without leaks, native code with both runtimes agrees, the
/// warnings match `.warn` (none when absent) and, when `CIRKA_GHC` is set,
/// GHC prints the same and exits with the same code.
#[test]
fn achc_run() {
    run_programs("achc-run", "run", &[0, 2]);
}

/// The differential corpus under tests/achc/diff/ (FEAT-0117): checked like
/// the run tests. The expectations were produced by GHC 9.4 (`-O0`) and are
/// compared with GHC again when `CIRKA_GHC` is set.
#[test]
fn achc_diff() {
    // Optimized builds only: the run tests cover -O0, and the interpreter
    // at -O0 makes the larger programs slow.
    run_programs("achc-diff", "diff", &[2]);
}

fn run_programs(suite: &str, sub: &str, default_levels: &[u8]) {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let root = format!("{}/", repo_root().display());
    // A directory with a `Main.hs` is one program of several modules.
    // A directory with a `Main.hs` (or `Main.chs`) is one program of
    // several modules.
    let is_main = |p: &std::path::Path| {
        p.file_name()
            .is_some_and(|n| n == "Main.hs" || n == "Main.chs")
    };
    let cases: Vec<_> = discover(&repo_root().join("tests/achc").join(sub), &["hs", "chs"])
        .into_iter()
        .filter(|c| {
            is_main(&c.path)
                || !c
                    .path
                    .ancestors()
                    .skip(1)
                    .take_while(|d| !d.ends_with(sub))
                    .any(|d| d.join("Main.hs").exists() || d.join("Main.chs").exists())
        })
        .collect();
    assert!(!cases.is_empty());
    run_suite(suite, &cases, |c| {
        let mut sm = SourceMap::new();
        let mut opts = cirka_driver::pipeline::Options::new(&c.name);
        // `-- flags: --ownership=…` are frontend options.
        opts.frontend_flags = c.header.flags();
        let roots = vec![c.path.parent().unwrap().to_path_buf(), achc::lib_dir()];
        let compiled = match achc::compile(&mut sm, &c.path, &roots, &opts, "test") {
            Ok(x) => x,
            Err(ds) => {
                return Outcome::Fail(
                    ds.iter()
                        .map(|d| render(d, &sm, Style::Plain))
                        .collect::<String>()
                        .replace(&root, ""),
                );
            }
        };
        let warn: String = compiled
            .warnings
            .iter()
            .map(|d| render(d, &sm, Style::Plain))
            .collect::<String>()
            .replace(&root, "");
        let wpath = c.sibling("warn");
        if (!warn.is_empty() || wpath.exists())
            && let Err(e) = check_expect(&wpath, &warn)
        {
            return Outcome::Fail(format!("warnings: {e}"));
        }
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let want_code: i32 = std::fs::read_to_string(c.sibling("exitcode"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap_or(0);
        let want_err = std::fs::read_to_string(c.sibling("stderr")).ok();
        // `-- ghc: skip` in the header: the program uses aCHC extensions
        // beyond GHC (e.g. inferred let multiplicities, FEAT-0120).
        let ghc_skip = std::fs::read_to_string(&c.path)
            .unwrap_or_default()
            .lines()
            .take(5)
            .any(|l| l.trim() == "-- ghc: skip")
            || c.path.extension().is_some_and(|e| e == "chs");
        if !ghc_skip && let Some(r) = cirka_testkit::ghc_reference(&c.path, Some(&stdin), &[]) {
            match r {
                Ok(g) => {
                    let want = std::fs::read_to_string(c.sibling("stdout")).unwrap_or_default();
                    if g.stdout != want || g.code != want_code {
                        return Outcome::Fail(format!(
                            "GHC disagrees with the expectation (exit {}):\n{}",
                            g.code,
                            cirka_testkit::line_diff(&want, &g.stdout)
                        ));
                    }
                    // GHC may print more call stack lines (from inside base).
                    if let Some(w) = &want_err {
                        let ge = normalize_stderr(&g.stderr, "prog: ", &root);
                        if !ge.starts_with(w.as_str()) {
                            return Outcome::Fail(format!(
                                "GHC's stderr does not start with the expectation:\n{}",
                                cirka_testkit::line_diff(w, &ge)
                            ));
                        }
                    }
                }
                // GHC is optional outside the Docker image.
                Err(e) if e.starts_with("cannot run") => {}
                Err(e) => return Outcome::Fail(e),
            }
        }
        let expect_leak = c.header.get("expect-leak").is_some();
        let mut reference = None;
        // `-- opt-levels: 2` limits a test to optimized builds (e.g. lazy
        // accumulators that need a deep stack at -O0, like GHC -O0).
        let levels: Vec<u8> = match c.header.get("opt-levels") {
            Some(l) => l
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect(),
            None => default_levels.to_vec(),
        };
        let last_level = levels.last().copied();
        for level in levels {
            let mut opts = cirka_driver::pipeline::Options::new(&c.name);
            opts.opt_level = level;
            opts.build_dir = cirka_testkit::work_dir("achc-run", &c.name);
            let out = match cirka_driver::pipeline::compile_core(
                compiled.core.clone(),
                &c.name,
                &opts,
                false,
            ) {
                Ok(o) => o,
                Err(ds) => {
                    return Outcome::Fail(
                        ds.iter()
                            .map(|d| d.message.clone())
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
            };
            let r = match cirka_mir::interp::run(&out.mir, &stdin, u64::MAX) {
                Ok(r) => r,
                Err(e) => return Outcome::Fail(format!("-O{level}: {e}")),
            };
            if let Err(e) = check_expect(&c.sibling("stdout"), &r.stdout) {
                return Outcome::Fail(format!("-O{level}: {e}"));
            }
            if r.exit_code != want_code {
                return Outcome::Fail(format!(
                    "-O{level}: exit code {} (expected {want_code})",
                    r.exit_code
                ));
            }
            if want_err.is_some() {
                let e = normalize_stderr(&r.stderr, "panic: ", &root);
                if let Err(e) = check_expect(&c.sibling("stderr"), &e) {
                    return Outcome::Fail(format!("-O{level}: stderr: {e}"));
                }
            }
            // `-- expect-leak: 1`: the program leaks (e.g. cyclic data
            // under `--cycles=warn`, FEAT-0126).
            if expect_leak && r.leaked == 0 {
                return Outcome::Fail(format!("-O{level}: no leak, but `expect-leak` is set"));
            }
            if r.leaked != 0 && want_code == 0 && !expect_leak {
                return Outcome::Fail(format!("-O{level}: {} objects leaked", r.leaked));
            }
            reference = Some((out.mir, r));
        }
        let (m, r) = reference.unwrap();
        // `-- same-cost-as: hand.hs` (FEAT-0061), compared at -O2.
        if c.header.get("same-cost-as").is_some() {
            if last_level != Some(2) {
                return Outcome::Fail("`same-cost-as` needs -O2 among the levels".into());
            }
            let hand = |p: &std::path::Path| {
                let mut sm = SourceMap::new();
                let roots = vec![p.parent().unwrap().to_path_buf(), achc::lib_dir()];
                let opts = cirka_driver::pipeline::Options::new("hand");
                let compiled = achc::compile(&mut sm, p, &roots, &opts, "test").map_err(|ds| {
                    ds.iter()
                        .map(|d| render(d, &sm, Style::Plain))
                        .collect::<String>()
                })?;
                cirka_integration::cost_at_o2(compiled.core, &c.name, &stdin)
            };
            let mine = cirka_integration::Cost::of(&m, &r);
            if let Err(e) = cirka_integration::check_same_cost(c, mine, hand) {
                return Outcome::Fail(e);
            }
        }
        cirka_integration::check_native_with(
            &cfg,
            c,
            "achc-native",
            &m,
            &stdin,
            &r,
            true,
            want_code == 0 && !expect_leak,
        )
    });
}

/// `--cycles=error` (FEAT-0126) rejects a recursive value binding (W0401
/// as an error); `--cycles=collect` accepts it without warnings.
#[test]
fn achc_cycles_flag() {
    let path = repo_root().join("tests/achc/run/h3/cycles_warn.hs");
    let roots = vec![path.parent().unwrap().to_path_buf(), achc::lib_dir()];
    let compile = |flag: &str| {
        let mut sm = SourceMap::new();
        let mut opts = cirka_driver::pipeline::Options::new("cycles");
        opts.frontend_flags = vec![flag.to_string()];
        achc::compile(&mut sm, &path, &roots, &opts, "cycles")
    };
    let errs = compile("--cycles=error").err().expect("rejected");
    assert!(
        errs.iter()
            .all(|d| d.code.as_deref() == Some("W0401") && d.is_error())
    );
    let ok = compile("--cycles=collect").unwrap_or_else(|_| panic!("rejected"));
    assert!(ok.warnings.is_empty());
    assert!(ok.core.collect_cycles);
    let warn = compile("--cycles=warn").unwrap_or_else(|_| panic!("rejected"));
    assert!(!warn.core.collect_cycles);
    assert_eq!(warn.warnings.len(), 1);
}

/// `achc.toml` packages (FEAT-0105): tests/achc/pkg/hello compiles through
/// the frontend of the `achc` driver and prints its greeting.
#[test]
fn achc_package() {
    let dir = repo_root().join("tests/achc/pkg/hello");
    let mut sm = SourceMap::new();
    let opts = cirka_driver::pipeline::Options::new("hello");
    let core = match achc::frontend(&mut sm, &dir, &opts) {
        Some(Ok(p)) => p,
        Some(Err(ds)) => panic!(
            "{}",
            ds.iter()
                .map(|d| render(d, &sm, Style::Plain))
                .collect::<String>()
        ),
        None => panic!("the package was not recognized"),
    };
    let out = cirka_driver::pipeline::compile_core(core, "hello", &opts, false).unwrap();
    let r = cirka_mir::interp::run(&out.mir, b"", u64::MAX).unwrap();
    assert_eq!(r.stdout, "hello, package\n");
    assert_eq!(r.leaked, 0);
    // Not a Haskell file: left to the driver (`.core`) or rejected.
    assert!(achc::frontend(&mut sm, &dir.join("achc.toml.bak"), &opts).is_none());
}

/// Stderr of a run with the program name prefix (`prog: ` of GHC's
/// binaries, `panic: ` of the interpreter) shown as `<prog>: ` and the
/// repository root removed from paths. GHC results are cached and may come
/// from a checkout elsewhere (`/work` in Docker), so any absolute path in
/// front of `tests/` is removed as well.
fn normalize_stderr(s: &str, prefix: &str, root: &str) -> String {
    s.lines()
        .map(|l| {
            let l = strip_checkout_paths(&l.replace(root, ""));
            match l.strip_prefix(prefix) {
                Some(rest) => format!("<prog>: {rest}\n"),
                None => format!("{l}\n"),
            }
        })
        .collect()
}

/// `... at /any/where/tests/x.hs:1:2` becomes `... at tests/x.hs:1:2`.
fn strip_checkout_paths(l: &str) -> String {
    l.split(' ')
        .map(|w| match w.find("/tests/") {
            Some(i) if w.starts_with('/') => &w[i + 1..],
            _ => w,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
