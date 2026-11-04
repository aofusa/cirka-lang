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
/// tests/achc/typeck/lib: `ok/*.hs` type check and the inferred bindings
/// of `Main` match `.tc`; `fail/*.hs` report the errors in `.stderr`.
#[test]
fn achc_typeck() {
    let dir = repo_root().join("tests/achc/typeck");
    let root = format!("{}/", repo_root().display());
    let mut failed = Vec::new();
    for (sub, ok) in [("ok", true), ("fail", false)] {
        let cases = discover(&dir.join(sub), &["hs"]);
        assert!(!cases.is_empty());
        for c in cases {
            let src = c.source();
            let mut sm = SourceMap::new();
            let id = sm.add(c.path.display().to_string(), src.clone());
            let mut loader = achc_rename::FsLoader {
                roots: vec![dir.join("lib")],
            };
            let opts = achc_rename::Options::default();
            let r = achc_rename::rename_program(&mut sm, id, &src, false, &mut loader, &opts)
                .and_then(|p| achc_typeck::typecheck(&p));
            let (ext, text) = match r {
                Ok(p) => ("tc", achc_typeck::tast::print_module_binds(&p, "Main")),
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
                let text: String = errs.iter().map(|d| render(d, &sm, Style::Plain)).collect();
                failed.push(format!("{}: {text}", c.name));
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
