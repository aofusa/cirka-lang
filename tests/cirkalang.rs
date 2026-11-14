//! CirkaLang front end tests: parse snapshots and syntax errors
//! (FEAT-0201).

use cirka_diagnostics::{SourceMap, Style, render};
use cirka_testkit::{Outcome, check_expect, discover, repo_root, run_suite};

fn render_all(name: &str, src: &str, errs: &[cirka_diagnostics::Diagnostic]) -> String {
    let mut sm = SourceMap::new();
    sm.add(name, src);
    errs.iter().map(|d| render(d, &sm, Style::Plain)).collect()
}

/// tests/cirkalang/parse/*.cirka parse without errors; the AST dump
/// matches `.ast`.
#[test]
fn cirkalang_parse_snapshots() {
    let cases = discover(&repo_root().join("tests/cirkalang/parse"), &["cirka"]);
    assert!(!cases.is_empty());
    run_suite("cirkalang-parse", &cases, |c| {
        let src = c.source();
        let (m, errs) = cirkalang_syntax::parse(cirka_diagnostics::FileId(0), &src);
        if !errs.is_empty() {
            return Outcome::Fail(render_all(&c.name, &src, &errs));
        }
        match check_expect(
            &c.sibling("ast"),
            &cirkalang_syntax::print::print_module(&m),
        ) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// tests/cirkalang/fail/syntax/*.cirka report the errors in `.stderr`.
#[test]
fn cirkalang_syntax_errors() {
    let cases = discover(&repo_root().join("tests/cirkalang/fail/syntax"), &["cirka"]);
    assert!(!cases.is_empty());
    run_suite("cirkalang-syntax-fail", &cases, |c| {
        let src = c.source();
        let (_, errs) = cirkalang_syntax::parse(cirka_diagnostics::FileId(0), &src);
        if errs.is_empty() {
            return Outcome::Fail("no syntax error reported".into());
        }
        match check_expect(
            &c.sibling("stderr"),
            &render_all(&format!("{}.cirka", c.name), &src, &errs),
        ) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// Multi-module programs under tests/cirkalang/modules/<name>/Main.cirka
/// (FEAT-0202): the resolved program matches `<name>.rn`, or the
/// diagnostics match `<name>.stderr` when that file exists.
#[test]
fn cirkalang_modules() {
    let dir = repo_root().join("tests/cirkalang/modules");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(!names.is_empty());
    let mut failed = Vec::new();
    for n in names {
        let main = dir.join(&n).join("Main.cirka");
        let mut sm = SourceMap::new();
        let opts = cirkalang_resolve::Options {
            prelude: false,
            ..Default::default()
        };
        let r = cirkalang_resolve::resolve_file(&mut sm, &main, &[], &opts);
        let root = format!("{}/", repo_root().display());
        let (path, text) = match r {
            Ok(p) => (
                dir.join(format!("{n}.rn")),
                cirkalang_resolve::print::print_program(&p),
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

/// Compiles a CirkaLang program to Core with the frontend of `cirka`.
fn compile_cirka(path: &std::path::Path) -> Result<(cirka_core::Program, String), String> {
    compile_cirka_with(path, cirka_driver::pipeline::SourceTests::Strip, &[])
}

fn compile_cirka_with(
    path: &std::path::Path,
    tests: cirka_driver::pipeline::SourceTests,
    flags: &[&str],
) -> Result<(cirka_core::Program, String), String> {
    let mut sm = SourceMap::new();
    let mut roots = vec![path.parent().unwrap().to_path_buf()];
    roots.extend(cirka::lib_roots());
    let mut opts = cirka_driver::pipeline::Options::new("test");
    opts.tests = tests;
    opts.frontend_flags = flags.iter().map(|f| f.to_string()).collect();
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
    opts.build_dir = cirka_testkit::work_dir("cirkalang-compile", &name);
    let render_diags = |sm: &SourceMap, ds: &[cirka_diagnostics::Diagnostic]| {
        let root = repo_root().display().to_string() + "/";
        ds.iter()
            .map(|d| render(d, sm, Style::Plain))
            .collect::<String>()
            .replace(&root, "")
    };
    match cirka::compile(&mut sm, path, &roots, &opts, &name) {
        Ok(c) => {
            let w = render_diags(&sm, &c.warnings);
            Ok((c.core, w))
        }
        Err(ds) => Err(render_diags(&sm, &ds)),
    }
}

/// CirkaLang programs under tests/cirkalang/run/v<version>/ (FEAT-0205):
/// Core → MIR → Perceus at -O0 and -O2, the interpreter prints `.stdout`
/// (exit code `.exitcode`, default 0) without leaks, and native code with
/// the debug and release runtimes agrees.
#[test]
fn cirkalang_run() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let mut cases = Vec::new();
    for v in std::fs::read_dir(repo_root().join("tests/cirkalang/run")).unwrap() {
        cases.extend(discover(&v.unwrap().path(), &["cirka"]));
    }
    assert!(!cases.is_empty());
    run_suite("cirkalang-run", &cases, |c| {
        let (core, warnings) = match compile_cirka(&c.path) {
            Ok(x) => x,
            Err(e) => return Outcome::Fail(e),
        };
        if !warnings.is_empty() {
            return Outcome::Fail(format!("unexpected warnings:\n{warnings}"));
        }
        // Normal builds contain no tests (FEAT-0216).
        if let Some(b) = core.binds.iter().find(|b| b.name.contains("test$")) {
            return Outcome::Fail(format!("test `{}` left in a normal build", b.name));
        }
        let stdin = std::fs::read(c.sibling("stdin")).unwrap_or_default();
        let want_code: i32 = std::fs::read_to_string(c.sibling("exitcode"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap_or(0);
        let mut reference = None;
        for level in [0u8, 2] {
            let mut opts = cirka_driver::pipeline::Options::new(&c.name);
            opts.opt_level = level;
            opts.build_dir = cirka_testkit::work_dir("cirkalang-run", &c.name);
            let out =
                match cirka_driver::pipeline::compile_core(core.clone(), &c.name, &opts, false) {
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
            // `panic` and `exit` end the process without unwinding: objects
            // alive at that point are not leaks (05-runtime.md).
            if r.leaked != 0 && want_code == 0 {
                return Outcome::Fail(format!("-O{level}: {} objects leaked", r.leaked));
            }
            reference = Some((out.mir, r));
        }
        let (m, r) = reference.unwrap();
        cirka_integration::check_native_with(
            &cfg,
            c,
            "cirkalang-native",
            &m,
            &stdin,
            &r,
            true,
            want_code == 0,
        )
    });
}

/// Ill-typed programs under tests/cirkalang/fail/{type,own,test,total,proof,refine}/ report
/// the diagnostics in `.stderr` (`test/` is checked with its tests kept, as
/// by `cirka check`).
#[test]
fn cirkalang_type_errors() {
    let mut cases = Vec::new();
    for d in ["type", "own", "test", "total", "proof", "refine"] {
        cases.extend(discover(
            &repo_root().join("tests/cirkalang/fail").join(d),
            &["cirka"],
        ));
    }
    assert!(!cases.is_empty());
    run_suite("cirkalang-type-fail", &cases, |c| {
        let tests = if c.path.parent().is_some_and(|d| d.ends_with("test")) {
            cirka_driver::pipeline::SourceTests::Check
        } else {
            cirka_driver::pipeline::SourceTests::Strip
        };
        let out = match compile_cirka_with(&c.path, tests, &[]) {
            Ok(_) => return Outcome::Fail("the program was accepted".into()),
            Err(e) => e,
        };
        match check_expect(&c.sibling("stderr"), &out) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// Proofs cost nothing (FEAT-0231): a program with proofs (an erased
/// `So` argument, a `rewrite`) compiles to the same MIR at -O2 as the same
/// program without them.
#[test]
fn cirkalang_proofs_are_erased() {
    let root = repo_root().join("tests/cirkalang/run/v004");
    let mir = |name: &str| {
        let (core, _) = compile_cirka(&root.join(format!("{name}.cirka"))).unwrap();
        let mut opts = cirka_driver::pipeline::Options::new("same");
        opts.opt_level = 2;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-erased", name);
        let out = cirka_driver::pipeline::compile_core(core, "same", &opts, false)
            .unwrap_or_else(|_| panic!("{name} does not compile"));
        cirka_mir::print::print_module(&out.mir)
    };
    let with = mir("proof_erased");
    let without = mir("proof_none");
    assert!(
        with == without,
        "the proofs left code behind:\n{with}\n----\n{without}"
    );
}

/// `--contracts` (FEAT-0233): `off` generates the same code as the
/// program without contracts; `check` turns what is not proved into run
/// time checks that panic with the contract (without Z3, all of them).
#[test]
fn cirkalang_contracts_modes() {
    let root = repo_root().join("tests/cirkalang/contracts");
    let mir = |name: &str, flags: &[&str]| {
        let (core, _) = compile_cirka_with(
            &root.join(format!("{name}.cirka")),
            cirka_driver::pipeline::SourceTests::Strip,
            flags,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut opts = cirka_driver::pipeline::Options::new("same");
        opts.opt_level = 2;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-contracts", name);
        cirka_driver::pipeline::compile_core(core, "same", &opts, false)
            .unwrap_or_else(|_| panic!("{name} does not compile"))
            .mir
    };
    let print = cirka_mir::print::print_module;
    let off = print(&mir("modes", &["--contracts=off"]));
    let none = print(&mir("modes_none", &[]));
    assert!(
        off == none,
        "--contracts=off left code behind:\n{off}\n----\n{none}"
    );
    let checked = mir("modes", &["--contracts=check"]);
    let ok = cirka_mir::interp::run(&checked, b"30\n", u64::MAX).unwrap();
    assert_eq!(ok.stdout, "10\n20\n");
    let bad = cirka_mir::interp::run(&checked, b"3\n", u64::MAX).unwrap();
    assert_ne!(bad.exit_code, 0);
    assert!(
        bad.stderr
            .contains("contract violated in `Main.main`: 10 <= n"),
        "{}",
        bad.stderr
    );
    let err = compile_cirka_with(
        &root.join("modes.cirka"),
        cirka_driver::pipeline::SourceTests::Strip,
        &["--contracts=maybe"],
    )
    .unwrap_err();
    assert!(err.contains("unknown contracts mode"), "{err}");
}

/// Checks removed by proofs (FEAT-0234): with the refinements proved or
/// checked at run time, `A.index`, `divNZ` and `unreachable` compile to
/// the operations without checks; with `--contracts=off`, to the checked
/// ones.
#[test]
fn cirkalang_proofs_remove_checks() {
    let path = repo_root().join("tests/cirkalang/contracts/proved.cirka");
    let mir = |flags: &[&str]| {
        let (core, _) =
            compile_cirka_with(&path, cirka_driver::pipeline::SourceTests::Strip, flags)
                .unwrap_or_else(|e| panic!("{e}"));
        let mut opts = cirka_driver::pipeline::Options::new("proved");
        opts.opt_level = 2;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-proved", &flags.join(""));
        cirka_driver::pipeline::compile_core(core, "proved", &opts, false)
            .unwrap_or_else(|_| panic!("does not compile"))
            .mir
    };
    // Without Z3, `check` checks the refinements at run time; with Z3,
    // `verify` proves them.
    let trusted = if cirka_smt::Solver::from_env().is_some() {
        mir(&[])
    } else {
        mir(&["--contracts=check"])
    };
    // The accesses of `search` (which the loop analysis of FEAT-0069 cannot
    // prove) and the division of `avg` (`""` is the whole module).
    let func = |m: &cirka_mir::Module, name: &str| -> String {
        let text = cirka_mir::print::print_module(m);
        text.split("\nfn ")
            .filter(|f| f.starts_with(name))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let search = func(&trusted, "Main.search");
    assert!(search.contains("prim array_get_unchecked("), "{search}");
    assert!(!search.contains("prim array_get("), "{search}");
    // `avg` may be inlined: nothing else divides by a proved divisor.
    assert!(func(&trusted, "").contains("prim div_nz("));
    let r = cirka_mir::interp::run(&trusted, b"", u64::MAX).unwrap();
    assert_eq!(r.stdout, "3\n3\n-1\n");
    let off = mir(&["--contracts=off"]);
    let search = func(&off, "Main.search");
    assert!(
        search.contains("prim array_get(") && !search.contains("_unchecked("),
        "{search}"
    );
    assert!(!func(&off, "").contains("div_nz("));
}

/// Termination by measures, decided by Z3 (FEAT-0230): the programs under
/// tests/cirkalang/verify/pass/ are accepted and print `.stdout`; those
/// under verify/fail/ report the diagnostics in `.stderr`. Skipped when Z3
/// is not installed (it is in the Docker image).
#[test]
fn cirkalang_verify() {
    if cirka_smt::Solver::from_env().is_none() {
        eprintln!("z3 not found (CIRKA_Z3): skipped");
        return;
    }
    let root = repo_root().join("tests/cirkalang/verify");
    let pass = discover(&root.join("pass"), &["cirka"]);
    assert!(!pass.is_empty());
    run_suite("cirkalang-verify-pass", &pass, |c| {
        let (core, _) = match compile_cirka(&c.path) {
            Ok(x) => x,
            Err(e) => return Outcome::Fail(e),
        };
        let mut opts = cirka_driver::pipeline::Options::new(&c.name);
        opts.build_dir = cirka_testkit::work_dir("cirkalang-verify", &c.name);
        let out = match cirka_driver::pipeline::compile_core(core, &c.name, &opts, false) {
            Ok(o) => o,
            Err(ds) => {
                return Outcome::Fail(format!(
                    "{:?}",
                    ds.iter().map(|d| &d.message).collect::<Vec<_>>()
                ));
            }
        };
        match cirka_mir::interp::run(&out.mir, &[], u64::MAX) {
            Ok(r) => match check_expect(&c.sibling("stdout"), &r.stdout) {
                Ok(()) => Outcome::Pass,
                Err(e) => Outcome::Fail(e),
            },
            Err(e) => Outcome::Fail(e.to_string()),
        }
    });
    let fail = discover(&root.join("fail"), &["cirka"]);
    assert!(!fail.is_empty());
    run_suite("cirkalang-verify-fail", &fail, |c| {
        let out = match compile_cirka(&c.path) {
            Ok(_) => return Outcome::Fail("the program was accepted".into()),
            Err(e) => e,
        };
        match check_expect(&c.sibling("stderr"), &out) {
            Ok(()) => Outcome::Pass,
            Err(e) => Outcome::Fail(e),
        }
    });
}

/// Traits leave no dictionaries behind in optimized code (FEAT-0210): no
/// dictionary is built and no method is selected at run time for programs
/// that use traits at known types.
#[test]
fn cirkalang_traits_specialize_away() {
    let root = repo_root().join("tests/cirkalang/run/v002");
    for name in [
        "trait_basic",
        "trait_context",
        "trait_super",
        "trait_num",
        "trait_generic_loop",
        "derive",
    ] {
        let (mut core, _) = compile_cirka(&root.join(format!("{name}.cirka"))).unwrap();
        cirka_core::opt::optimize(&mut core).unwrap();
        cirka_core::opt::dce_globals(&mut core);
        // The declarations of the dictionary types stay.
        let text = cirka_core::print_program(&core);
        let left: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with("(data ") && l.contains("$Dict"))
            .map(|l| l.trim())
            .collect();
        assert!(
            left.is_empty(),
            "{name}: dictionaries remain:\n{}",
            left.join("\n")
        );
        // A generic loop at a known type builds no boxes.
        if name == "trait_generic_loop" {
            let workers: Vec<&str> = text
                .split("\n(def ")
                .filter(|d| d.starts_with("Main.loop"))
                .collect();
            assert!(!workers.is_empty(), "{text}");
            for w in workers {
                assert!(
                    w.contains('#') && !w.contains("(con "),
                    "boxes in the loop:\n{w}"
                );
            }
        }
    }
}

/// A borrow costs nothing at run time (FEAT-0220, spec-v0.0.3.md §2.5):
/// insertion sort through `Borrow` makes no more allocations or
/// reference-count operations than the same loop threading the array by
/// hand (up to a constant for entering the borrow).
#[test]
fn cirkalang_borrow_costs_nothing() {
    let root = repo_root().join("tests/cirkalang/run/v003");
    let stats = |name: &str| {
        let (core, _) = compile_cirka(&root.join(format!("{name}.cirka"))).unwrap();
        let mut opts = cirka_driver::pipeline::Options::new(name);
        opts.opt_level = 2;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-borrow", name);
        let out = cirka_driver::pipeline::compile_core(core, name, &opts, false)
            .unwrap_or_else(|_| panic!("{name} does not compile"));
        let r = cirka_mir::interp::run(&out.mir, &[], u64::MAX).unwrap();
        assert_eq!(r.leaked, 0, "{name}");
        r.stats
    };
    let b = stats("borrow_loop");
    let o = stats("owned_loop");
    // Entering the borrow may cost a constant few; the loop (22629 swaps)
    // nothing.
    assert!(
        b.allocs <= o.allocs + 4 && b.rc_ops <= o.rc_ops + 4,
        "borrowed: {b:?}\nowned: {o:?}"
    );
}

/// Projects of several packages under tests/cirkalang/packages/<case>/
/// (FEAT-0214): `<case>/app` is built with its dependencies; it prints
/// `<case>.stdout`, or is rejected with the diagnostics in `<case>.stderr`.
#[test]
fn cirkalang_packages() {
    let dir = repo_root().join("tests/cirkalang/packages");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
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
        let mut sm = SourceMap::new();
        let opts = cirka_driver::pipeline::Options::new("test");
        let res = match cirka::compile_package(&mut sm, &dir.join(&n).join("app"), &opts) {
            Ok(c) => {
                let mut o = cirka_driver::pipeline::Options::new(&n);
                o.build_dir = cirka_testkit::work_dir("cirkalang-packages", &n);
                match cirka_driver::pipeline::compile_core(c.core, &n, &o, false) {
                    Ok(out) => match cirka_mir::interp::run(&out.mir, &[], u64::MAX) {
                        Ok(r) if r.leaked == 0 => {
                            check_expect(&dir.join(format!("{n}.stdout")), &r.stdout)
                        }
                        Ok(r) => Err(format!("{} objects leaked", r.leaked)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(ds) => Err(ds
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("\n")),
                }
            }
            Err(errs) => check_expect(
                &dir.join(format!("{n}.stderr")),
                &errs
                    .iter()
                    .map(|d| render(d, &sm, Style::Plain))
                    .collect::<String>()
                    .replace(&root, ""),
            ),
        };
        if let Err(e) = res {
            failed.push(format!("{n}: {e}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    // A library (no `src/Main.cirka`) is checked and tested with all its
    // modules, but cannot be built as a program.
    let lib = dir.join("diamond/util");
    for (mode, ok) in [
        (cirka_driver::pipeline::SourceTests::Check, true),
        (cirka_driver::pipeline::SourceTests::Run, true),
        (cirka_driver::pipeline::SourceTests::Strip, false),
    ] {
        let mut sm = SourceMap::new();
        let mut opts = cirka_driver::pipeline::Options::new("test");
        opts.tests = mode;
        match cirka::compile_package(&mut sm, &lib, &opts) {
            Ok(c) => {
                assert!(ok, "{mode:?}: a library was built as a program");
                if mode == cirka_driver::pipeline::SourceTests::Run {
                    assert!(c.core.binds.iter().any(|b| b.name.contains("test$")));
                }
            }
            Err(e) => {
                assert!(!ok, "{mode:?}: {}", e[0].message);
                assert_eq!(e[0].code.as_deref(), Some("E1107"));
            }
        }
    }
}
