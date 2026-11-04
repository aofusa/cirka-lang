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
        let opts = cirkalang_resolve::Options { prelude: false };
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
