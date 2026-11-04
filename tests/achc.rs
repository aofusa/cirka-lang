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
