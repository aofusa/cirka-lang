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
/// the debug and release runtimes agrees. Programs with threads, which the
/// interpreter does not run, say `-- skip-backend: interp` (spec-v0.0.5.md
/// §1.5): native code alone prints `.stdout`. A `.c` file next to a
/// program is its C helper (spec-v0.0.6.md §1), loaded by the interpreter
/// and linked with native code. A program with `-- requires:` is skipped
/// without what it names (`vulkan`: glslc and a Vulkan device, spec-v0.0.6.md
/// §3; `lib:<name>`: a C library, §4); the shaders `<name>.<shader>.comp`
/// of a program are compiled for its C helper.
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
        if let Some(why) = cirka_integration::missing_requirement(c) {
            return Outcome::Skip(why);
        }
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
        let interp_ok = c.header.get("skip-backend") != Some("interp");
        let link_args = match cirka_integration::c_helper(&cfg, c, "cirkalang-run") {
            Ok(a) => a,
            Err(e) => return Outcome::Fail(e),
        };
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
            if !interp_ok {
                let r = cirka_mir::interp::RunResult {
                    stdout: std::fs::read_to_string(c.sibling("stdout")).unwrap_or_default(),
                    stderr: String::new(),
                    exit_code: want_code,
                    stats: Default::default(),
                    leaked: 0,
                    orderly: want_code == 0,
                };
                reference = Some((out.mir, r));
                continue;
            }
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
            // alive at that point are not leaks (05-runtime.md §6.1), unless
            // an uncaught exception unwound (FEAT-0040).
            if r.leaked != 0 && (want_code == 0 || r.orderly) {
                return Outcome::Fail(format!("-O{level}: {} objects leaked", r.leaked));
            }
            reference = Some((out.mir, r));
        }
        let (m, r) = reference.unwrap();
        if interp_ok && let Err(e) = cirka_integration::check_max_rc_ops(c, &r) {
            return Outcome::Fail(format!("-O2: {e}"));
        }
        // `-- same-cost-as: hand.cirka` (FEAT-0061), compared at -O2.
        let hand = |p: &std::path::Path| {
            let (core, _) = compile_cirka(p)?;
            cirka_integration::cost_at_o2(core, &c.name, &stdin)
        };
        if interp_ok
            && let Err(e) =
                cirka_integration::check_same_cost(c, cirka_integration::Cost::of(&m, &r), hand)
        {
            return Outcome::Fail(e);
        }
        cirka_integration::check_native_linked(
            &cfg,
            c,
            "cirkalang-native",
            &m,
            &stdin,
            &r,
            interp_ok,
            want_code == 0 || r.orderly,
            &link_args,
        )
    });
}

/// Ill-typed programs under tests/cirkalang/fail/{type,own,test,total,proof,refine,thread,net,vulkan,linalg,cap,effect}/ report
/// the diagnostics in `.stderr` (`test/` is checked with its tests kept, as
/// by `cirka check`).
#[test]
fn cirkalang_type_errors() {
    let mut cases = Vec::new();
    for d in [
        "type", "own", "test", "total", "proof", "refine", "thread", "net", "vulkan", "linalg",
        "cap", "effect", "session",
    ] {
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

/// Capabilities cost nothing (FEAT-0260, spec-v0.0.7.md §1.6): a program
/// that passes an erased capability compiles to the same MIR at -O2 as the
/// same program without it.
#[test]
fn cirkalang_capabilities_are_erased() {
    let root = repo_root().join("tests/cirkalang/run/v007");
    let mir = |name: &str| {
        let (core, _) = compile_cirka(&root.join(format!("{name}.cirka"))).unwrap();
        let mut opts = cirka_driver::pipeline::Options::new("same");
        opts.opt_level = 2;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-caps-erased", name);
        let out = cirka_driver::pipeline::compile_core(core, "same", &opts, false)
            .unwrap_or_else(|_| panic!("{name} does not compile"));
        cirka_mir::print::print_module(&out.mir)
    };
    let with = mir("cap_erased");
    let without = mir("cap_none");
    assert!(
        with == without,
        "the capabilities left code behind:\n{with}\n----\n{without}"
    );
}

/// The items of extern blocks that a program does not use are neither
/// elaborated (struct and union code) nor lowered (FEAT-0255,
/// spec-v0.0.6.md §1.10); `--extern-items=all` keeps all of them.
#[test]
fn cirkalang_extern_items_on_demand() {
    let path = repo_root().join("tests/cirkalang/extern/OnDemand.cirka");
    let names = |flags: &[&str]| {
        let (core, warnings) =
            compile_cirka_with(&path, cirka_driver::pipeline::SourceTests::Strip, flags)
                .unwrap_or_else(|e| panic!("OnDemand.cirka does not compile:\n{e}"));
        assert_eq!(warnings, "");
        core.binds
            .iter()
            .map(|b| b.name.clone())
            .collect::<std::collections::HashSet<_>>()
    };
    let used = names(&[]);
    let all = names(&["--extern-items=all"]);
    // `Storable P` (with `P.$peek`) is used with `P`.
    for n in ["Main.P.x", "Main.P.$peek", "Main.main"] {
        assert!(used.contains(n), "`{n}` is used but missing");
    }
    for n in [
        "Main.P.y",
        "Main.P.set_x",
        "Main.Q.a",
        "Main.Q.$poke",
        "Main.K",
        "Main.A",
    ] {
        assert!(!used.contains(n), "`{n}` is not used but lowered");
        assert!(all.contains(n), "`{n}` is missing with --extern-items=all");
    }
}

/// `--contracts` (FEAT-0233): `off` generates the same code as the
/// program without contracts; `check` turns what is not proved into run
/// time checks that panic with the contract (without Z3, all of them).
/// BUG-0017: a run-time checked invariant that compares with a trait
/// method and `&&` compiles (no ICE) and has no coverage warning.
#[test]
fn cirkalang_invariant_with_trait_methods() {
    let path = repo_root().join("tests/cirkalang/contracts/invariant_ord.cirka");
    for mode in ["--contracts=off", "--contracts=check"] {
        let (core, warnings) =
            compile_cirka_with(&path, cirka_driver::pipeline::SourceTests::Strip, &[mode])
                .unwrap_or_else(|e| panic!("{mode}: {e}"));
        assert!(!warnings.contains("W1231"), "{mode}: {warnings}");
        let mut opts = cirka_driver::pipeline::Options::new("inv");
        opts.build_dir = cirka_testkit::work_dir("cirkalang-invariant", mode);
        let mir = cirka_driver::pipeline::compile_core(core, "inv", &opts, false)
            .unwrap_or_else(|_| panic!("{mode}: does not compile"))
            .mir;
        let ok = cirka_mir::interp::run(&mir, b"2\n", u64::MAX).unwrap();
        assert_eq!(ok.stdout, "6\n", "{mode}");
        if mode == "--contracts=check" {
            let bad = cirka_mir::interp::run(&mir, b"5\n", u64::MAX).unwrap();
            assert_ne!(bad.exit_code, 0, "{}", bad.stdout);
            assert!(bad.stderr.contains("contract violated"), "{}", bad.stderr);
        }
    }
}

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

/// The MIR (at -O2) of the run test `name` (v006) compiled with `flags`.
fn mir_of(name: &str, flags: &[&str]) -> String {
    let path = repo_root().join(format!("tests/cirkalang/run/v006/{name}.cirka"));
    let (core, _) = compile_cirka_with(&path, cirka_driver::pipeline::SourceTests::Strip, flags)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut opts = cirka_driver::pipeline::Options::new(name);
    opts.opt_level = 2;
    opts.build_dir = cirka_testkit::work_dir(&format!("cirkalang-{name}"), &flags.join(""));
    let m = cirka_driver::pipeline::compile_core(core, name, &opts, false)
        .unwrap_or_else(|_| panic!("does not compile"))
        .mir;
    cirka_mir::print::print_module(&m)
}

/// The accesses that proofs make without checks (FEAT-0254,
/// spec-v0.0.6.md §5): in the MIR of the run test `name`, the functions
/// whose names start with each of `funcs` (with their workers) use the
/// operations `ops` without bounds checks, and no checked one, while under
/// `--contracts=off` they are checked. (The loop analysis of FEAT-0069
/// still removes the checks of a simple loop behind a test before the
/// loop under `off`, keeping the checked loop for when the test fails.)
/// With Z3 the indices are proved, so these functions check nothing at
/// run time; without it, `check` checks the refinements at run time
/// instead. Returns how long the compilation with the proofs took.
fn proofs_remove_checks(name: &str, funcs: &[(&str, &[&str])]) -> std::time::Duration {
    let z3 = cirka_smt::Solver::from_env().is_some();
    let func = |text: &str, prefix: &str| -> String {
        text.split("\nfn ")
            .filter(|f| f.starts_with(prefix))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let start = std::time::Instant::now();
    let trusted = if z3 {
        mir_of(name, &[])
    } else {
        mir_of(name, &["--contracts=check"])
    };
    let took = start.elapsed();
    let off = mir_of(name, &["--contracts=off"]);
    for (prefix, ops) in funcs {
        let t = func(&trusted, prefix);
        let o = func(&off, prefix);
        assert!(!t.is_empty(), "no function {prefix} in {name}");
        for op in *ops {
            assert!(t.contains(&format!("prim {op}_unchecked(")), "{op}: {t}");
            assert!(!t.contains(&format!("prim {op}(")), "{op}: {t}");
            assert!(o.contains(&format!("prim {op}(")), "{op}: {o}");
        }
        if z3 {
            assert!(!t.contains("contract violated"), "{t}");
        }
    }
    took
}

/// Unique arrays with their size in the type: the loops of
/// sized_arrays.cirka read and write without bounds checks.
#[test]
fn cirkalang_sized_arrays_remove_checks() {
    let t = proofs_remove_checks(
        "sized_arrays",
        &[
            ("Main.squares", &["bytes_set_i64"]),
            ("Main.prefix", &["bytes_get_f64", "bytes_set_f64"]),
            ("Main.reverse", &["array_get", "array_set"]),
        ],
    );
    // The checks of the refinements take a fraction of a second (BUG-0029:
    // 76 s, evaluating the loops that fill the arrays).
    assert!(
        t < std::time::Duration::from_secs(30),
        "compiled in {t:.2?} (BUG-0029)"
    );
}

/// Views of strings: the reads of string_views.cirka and the loops of
/// `Data.String` (whose views are within their strings, an invariant of
/// `Str`) have no bounds checks.
#[test]
fn cirkalang_string_views_remove_checks() {
    proofs_remove_checks(
        "string_views",
        &[
            ("Main.sumBytes", &["bytes_get"]),
            ("Main.palindrome", &["bytes_get"]),
            ("Data.String.skipSpace", &["bytes_get"]),
            ("Data.String.skipWord", &["bytes_get"]),
        ],
    );
}

/// The proof obligations of the libraries are not checked when a program
/// is compiled (FEAT-0244, spec-v0.0.5.md §4.3): they are checked here,
/// with `--libraries=verify`, on a program that imports every module of
/// the libraries. Skipped when Z3 is not installed (it is in the Docker
/// image).
#[test]
fn cirkalang_verify_libraries() {
    if cirka_smt::Solver::from_env().is_none() {
        eprintln!("z3 not found (CIRKA_Z3): skipped");
        return;
    }
    fn modules(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                modules(root, &p, out);
            } else if p.extension().is_some_and(|x| x == "cirka") {
                let rel = p.strip_prefix(root).unwrap().with_extension("");
                let segs: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(segs.join("."));
            }
        }
    }
    let mut all = Vec::new();
    for root in cirka::lib_roots() {
        modules(&root, &root, &mut all);
    }
    all.sort();
    assert!(all.iter().any(|m| m == "Data.Array"), "{all:?}");
    let mut src = String::new();
    for (k, m) in all.iter().enumerate() {
        if m != "Prelude" {
            src.push_str(&format!("import {m} as L{k}\n"));
        }
    }
    src.push_str("\nmain : IO ()\nmain = pure ()\n");
    let dir = cirka_testkit::work_dir("cirkalang-verify-libraries", "all");
    let path = dir.join("Libraries.cirka");
    std::fs::write(&path, src).unwrap();
    let r = compile_cirka_with(
        &path,
        cirka_driver::pipeline::SourceTests::Strip,
        &["--libraries=verify"],
    );
    match r {
        Ok((_, warnings)) => assert!(warnings.is_empty(), "{warnings}"),
        Err(e) => panic!("the libraries' obligations do not hold:\n{e}"),
    }
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

/// `cirka$bytes_write` of the support library (FEAT-0097) on the ranges
/// the libraries never pass it (`MkStr`'s invariant keeps a view's range in
/// its string; the run test `string_write_clamp` covers a `Builder`'s
/// offsets): over offsets and indices around the ends of the destination
/// and the source and at the extremes, where `j - i` and `len - off` wrap
/// around, a range that fits is copied inline and any other is clamped,
/// both as the model of the clamping says, at -O0 and -O2, in the
/// interpreter and in native code.
#[test]
fn cirkalang_support_bytes_write_clamps() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let vals = [i64::MIN, -1, 0, 1, 3, 4, 6, 7, i64::MAX];
    // "abc"[i..j] clamped to the string, written over "......" at `off`
    // clamped to it, as much as there is room for.
    let model = |off: i64, i: i64, j: i64| {
        let (mut u, s) = (*b"......", b"abc");
        let a = i.clamp(0, 3);
        let b = j.clamp(a, 3);
        let o = off.clamp(0, 6);
        let n = (b - a).min(6 - o) as usize;
        let (a, o) = (a as usize, o as usize);
        u[o..o + n].copy_from_slice(&s[a..a + n]);
        String::from_utf8(u.to_vec()).unwrap() + "\n"
    };
    let mut expected = String::new();
    for &off in &vals {
        for &i in &vals {
            for &j in &vals {
                expected += &model(off, i, j);
            }
        }
    }
    let lit = |v: i64| match v {
        i64::MIN => format!("(prim sub {} 1)", i64::MIN + 1),
        v => v.to_string(),
    };
    let alts: Vec<String> = vals
        .iter()
        .enumerate()
        .map(|(k, &v)| match k {
            8 => format!("(_ {})", lit(v)),
            k => format!("({k} {})", lit(v)),
        })
        .collect();
    let src = format!(
        "{}
(def grid$val (-> Int# Int#)
  (lam ((k Int#)) (case k (c Int#) Int# {})))
(def grid$one (-> Int# Int# Int# Int#)
  (lam ((off Int#) (i Int#) (j Int#))
    (let! (u Bytes (cirka$bytes_copy_at (prim bytes_new 6) 0 \"......\" 0 6))
    (let! (v Bytes (cirka$bytes_write u off \"abc\" i j))
    (let! (w Unit# (extern cirka_write_bytes v))
    (let! (x Unit# (extern cirka_write_char 10))
      0))))))
(def grid$go (-> Int# Int#)
  (lam ((k Int#))
    (case (prim lt k 729) (c Int#) Int#
      (0 0)
      (_ (let! (r Int# (grid$one (grid$val (prim quot k 81))
                                 (grid$val (prim rem (prim quot k 9) 9))
                                 (grid$val (prim rem k 9))))
           (grid$go (prim add k 1)))))))
(def main Int# (grid$go 0))
",
        include_str!("../crates/cirkalang-lower/src/support.core"),
        alts.join(" ")
    );
    let core = cirka_core::parse_program(cirka_diagnostics::FileId(0), &src)
        .unwrap_or_else(|e| panic!("{}", e.message));
    for level in [0u8, 2] {
        let name = format!("bytes_write_O{level}");
        let mut opts = cirka_driver::pipeline::Options::new(&name);
        opts.opt_level = level;
        opts.build_dir = cirka_testkit::work_dir("cirkalang-support", &name);
        let out = cirka_driver::pipeline::compile_core(core.clone(), &name, &opts, false)
            .unwrap_or_else(|ds| panic!("-O{level}: {}", ds[0].message));
        let r = cirka_mir::interp::run(&out.mir, &[], u64::MAX)
            .unwrap_or_else(|e| panic!("-O{level}: {e}"));
        assert_eq!(r.exit_code, 0, "-O{level}: {}", r.stderr);
        assert_eq!(r.leaked, 0, "-O{level}");
        if r.stdout != expected {
            panic!(
                "-O{level}: the interpreter differs from the model\n{}",
                cirka_testkit::line_diff(&expected, &r.stdout)
            );
        }
        let case = cirka_testkit::Case {
            path: repo_root().join("crates/cirkalang-lower/src/support.core"),
            name,
            header: Default::default(),
        };
        match cirka_integration::check_native(
            &cfg,
            &case,
            "cirkalang-support-native",
            &out.mir,
            &[],
            &r,
            true,
        ) {
            Outcome::Fail(e) => panic!("-O{level}: {e}"),
            Outcome::Pass | Outcome::Skip(_) => {}
        }
    }
}
