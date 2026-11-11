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
    compile_cirka_with(path, cirka_driver::pipeline::SourceTests::Strip)
}

fn compile_cirka_with(
    path: &std::path::Path,
    tests: cirka_driver::pipeline::SourceTests,
) -> Result<(cirka_core::Program, String), String> {
    let mut sm = SourceMap::new();
    let mut roots = vec![path.parent().unwrap().to_path_buf()];
    roots.extend(cirka::lib_roots());
    let mut opts = cirka_driver::pipeline::Options::new("test");
    opts.tests = tests;
    let name = path.file_stem().unwrap().to_string_lossy().to_string();
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

/// Ill-typed programs under tests/cirkalang/fail/{type,own,test}/ report
/// the diagnostics in `.stderr` (`test/` is checked with its tests kept, as
/// by `cirka check`).
#[test]
fn cirkalang_type_errors() {
    let mut cases = Vec::new();
    for d in ["type", "own", "test"] {
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
        let out = match compile_cirka_with(&c.path, tests) {
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
