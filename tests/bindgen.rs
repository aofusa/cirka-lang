//! bindgen tests (FEAT-0048, 08-ffi.md §6).
//!
//! - tests/bindgen/gen/*.h: the declarations written for aCHC (`.hs`) and
//!   CirkaLang (`.cirka`), or clang's diagnostics (`.stderr`), which the
//!   front ends accept. The arguments of `bindgen` are in the
//!   `// bindgen:` comment.
//! - tests/bindgen/run/<case>/<name>.h: `achc bindgen` / `cirka bindgen`
//!   write the module `<Name>` from the header, which `Main.hs` /
//!   `Main.cirka` uses; each program prints `Main.stdout` (or its own
//!   `Main.hs.stdout` / `Main.cirka.stdout`). A `<name>.c`
//!   next to the header is a C library linked with the program, as are
//!   the `--link=` libraries. With `CIRKA_GHC`, GHC runs `Main.hs` too.
//! - Whole system headers (`math.h`, `vulkan_core.h`) are converted to
//!   both languages.
//!
//! A case with `// requires: <header>` is skipped when that header cannot
//! be included: the Vulkan headers are in the Docker image, and
//! scripts/ci.sh requires them there.

use cirka_bindgen::model::Def;
use cirka_bindgen::{Env, Lang};
use cirka_testkit::{Case, Outcome, check_expect, discover, repo_root, run_suite};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The arguments of `bindgen` for the header of a case: its `bindgen:`
/// comment, then the header.
fn bindgen_args(c: &Case) -> Vec<String> {
    let mut args: Vec<String> = c
        .header
        .get("bindgen")
        .unwrap_or_default()
        .split_whitespace()
        .map(String::from)
        .collect();
    args.push(c.path.display().to_string());
    args
}

/// Whether `#include <header>` compiles with the configured clang.
fn includable(cfg: &cirka_driver::Config, header: &str) -> bool {
    let child = Command::new(&cfg.clang)
        .args(["-fsyntax-only", "-x", "c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "#include <{header}>");
    }
    child.wait().is_ok_and(|s| s.success())
}

/// Why the case cannot run here (`requires:` a header that is missing).
fn missing_requirement(cfg: &cirka_driver::Config, c: &Case) -> Option<String> {
    let h = c.header.get("requires")?;
    (!includable(cfg, h)).then(|| format!("<{h}> is not installed"))
}

/// tests/bindgen/gen/*.h: the output for both languages, or the errors.
#[test]
fn bindgen_golden() {
    cirka_testkit::init();
    let env = Env::from_env();
    let root = format!("{}/", repo_root().display());
    let cases = discover(&repo_root().join("tests/bindgen/gen"), &["h"]);
    assert!(!cases.is_empty());
    run_suite("bindgen-gen", &cases, |c| {
        let a = match cirka_bindgen::cli::parse_args(&bindgen_args(c)) {
            Ok(a) => a,
            Err(e) => return Outcome::Fail(e),
        };
        let source = c.path.file_name().unwrap().to_string_lossy();
        let (hs, cirka, stderr) = match cirka_bindgen::generate(&env, &a.opts) {
            Ok(h) => (
                cirka_bindgen::print(&h, &a.opts, Lang::Haskell, &source),
                cirka_bindgen::print(&h, &a.opts, Lang::Cirka, &source),
                String::new(),
            ),
            Err(es) => (
                String::new(),
                String::new(),
                es.iter()
                    .map(|e| format!("{}\n", e.replace(&root, "")))
                    .collect(),
            ),
        };
        if !hs.is_empty()
            && let Err(e) = achc_accepts(c, a.opts.module.as_deref().unwrap_or("Bindings"), &hs)
        {
            return Outcome::Fail(e);
        }
        if !cirka.is_empty()
            && let Err(e) = cirka_accepts(c, &cirka)
        {
            return Outcome::Fail(e);
        }
        for (ext, text) in [("hs", hs), ("cirka", cirka), ("stderr", stderr)] {
            if let Err(e) = check_expect(&c.sibling(ext), &text) {
                return Outcome::Fail(e);
            }
        }
        Outcome::Pass
    });
}

/// The aCHC front end accepts the module `text` (imported by a `Main`).
fn achc_accepts(c: &Case, module: &str, text: &str) -> Result<(), String> {
    let work = cirka_testkit::work_dir("bindgen-gen", &c.name);
    let main = work.join("Main.hs");
    std::fs::write(work.join(format!("{module}.hs")), text).map_err(|e| e.to_string())?;
    std::fs::write(
        &main,
        format!("import {module} ()\n\nmain :: IO ()\nmain = return ()\n"),
    )
    .map_err(|e| e.to_string())?;
    let mut sm = cirka_diagnostics::SourceMap::new();
    let opts = cirka_driver::pipeline::Options::new(&c.name);
    let roots = vec![work.clone(), achc::lib_dir()];
    achc::compile(&mut sm, &main, &roots, &opts, "test")
        .map(|_| ())
        .map_err(|ds| {
            ds.iter()
                .map(|d| cirka_diagnostics::render(d, &sm, cirka_diagnostics::Style::Plain))
                .collect()
        })
}

/// The CirkaLang front end accepts the module `text` (spec-v0.0.6.md §1),
/// imported by a `Main`.
fn cirka_accepts(c: &Case, text: &str) -> Result<(), String> {
    let work = cirka_testkit::work_dir("bindgen-gen", &c.name);
    let main = work.join("Main.cirka");
    std::fs::write(work.join("Bindings.cirka"), text).map_err(|e| e.to_string())?;
    std::fs::write(
        &main,
        "module Main\n\nimport Bindings as B\n\nmain : IO ()\nmain = pure ()\n",
    )
    .map_err(|e| e.to_string())?;
    let mut sm = cirka_diagnostics::SourceMap::new();
    let mut opts = cirka_driver::pipeline::Options::new(&c.name);
    // Everything the declarations expand to is checked, used or not
    // (spec-v0.0.6.md §1.10).
    opts.frontend_flags.push("--extern-items=all".into());
    let mut roots = vec![work.clone()];
    roots.extend(cirka::lib_roots());
    cirka::compile(&mut sm, &main, &roots, &opts, "test")
        .map(|_| ())
        .map_err(|ds| {
            ds.iter()
                .map(|d| cirka_diagnostics::render(d, &sm, cirka_diagnostics::Style::Plain))
                .collect()
        })
}

fn run(bin: &Path, args: &[String], cwd: &Path) -> cirka_testkit::RunOutput {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd).env("CIRKA_LEAK_CHECK", "1");
    cirka_testkit::run_with_stdin(cmd, None).unwrap()
}

/// `Name` for the header `name.h`.
fn module_name(header: &Path) -> String {
    let stem = header.file_stem().unwrap().to_string_lossy();
    let mut c = stem.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

/// Generates the module of a run case with `TOOL bindgen` into `work`
/// next to a copy of the program, and runs the program at -O0 and -O2.
fn run_with(
    tool: &str,
    bin: &Path,
    ext: &str,
    c: &Case,
    work: &Path,
    link: &[String],
    expected: &str,
) -> Result<(), String> {
    let dir = c.path.parent().unwrap();
    let main = dir.join(format!("Main.{ext}"));
    let module = work.join(format!("{}.{ext}", module_name(&c.path)));
    let mut args = vec!["bindgen".to_string()];
    args.extend(bindgen_args(c));
    args.extend(["-o".to_string(), module.display().to_string()]);
    let out = run(bin, &args, work);
    if out.code != 0 {
        return Err(format!("{tool} bindgen: exit {}\n{}", out.code, out.stderr));
    }
    let program = work.join(format!("Main.{ext}"));
    std::fs::copy(&main, &program).map_err(|e| format!("cannot copy {}: {e}", main.display()))?;
    for (o, rt) in [("-O0", "--runtime=debug"), ("-O2", "--runtime=release")] {
        let mut args = vec![
            "run".to_string(),
            o.to_string(),
            rt.to_string(),
            format!("--build-dir={}", work.join(format!("build{o}")).display()),
        ];
        args.extend(link.iter().cloned());
        args.push(program.display().to_string());
        let out = run(bin, &args, work);
        if out.stdout != expected || out.code != 0 {
            return Err(format!(
                "{tool} {o}: exit {}\n{}\n{}",
                out.code,
                cirka_testkit::line_diff(expected, &out.stdout),
                out.stderr
            ));
        }
        if out.stderr.contains("leaked") {
            return Err(format!("{tool} {o}: {}", out.stderr));
        }
    }
    Ok(())
}

/// tests/bindgen/run/<case>/: programs using generated modules, with
/// both CLIs (and GHC).
#[test]
fn bindgen_run() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let (achc, cirka) = cirka_integration::cli_bins();
    let cases = discover(&repo_root().join("tests/bindgen/run"), &["h"]);
    assert!(!cases.is_empty());
    run_suite("bindgen-run", &cases, |c| {
        if let Some(why) = missing_requirement(&cfg, c) {
            return Outcome::Skip(why);
        }
        let opts = match cirka_bindgen::cli::parse_args(&bindgen_args(c)) {
            Ok(a) => a.opts,
            Err(e) => return Outcome::Fail(e),
        };
        let mut link = match cirka_integration::c_helper(&cfg, c, "bindgen-run") {
            Ok(l) => l,
            Err(e) => return Outcome::Fail(e),
        };
        link.extend(opts.links.iter().map(|l| format!("-l{l}")));
        let dir = c.path.parent().unwrap();
        let expected = std::fs::read_to_string(dir.join("Main.stdout")).unwrap_or_default();
        let mut programs = 0;
        for (tool, bin, ext) in [("achc", achc, "hs"), ("cirka", cirka, "cirka")] {
            if !dir.join(format!("Main.{ext}")).exists() {
                continue;
            }
            programs += 1;
            let work = cirka_testkit::work_dir("bindgen-run", &format!("{}-{tool}", c.name));
            let expected = std::fs::read_to_string(dir.join(format!("Main.{ext}.stdout")))
                .unwrap_or_else(|_| expected.clone());
            if let Err(e) = run_with(tool, bin, ext, c, &work, &link, &expected) {
                return Outcome::Fail(e);
            }
            // GHC runs the same program with the same generated module.
            if ext == "hs" {
                let mut ghc_args: Vec<String> = Vec::new();
                if c.sibling("c").exists() {
                    ghc_args.push(c.sibling("c").display().to_string());
                }
                ghc_args.extend(opts.links.iter().map(|l| format!("-l{l}")));
                match cirka_testkit::ghc_reference_with(&work.join("Main.hs"), None, &[], &ghc_args)
                {
                    Some(Ok(g)) if g.stdout != expected || g.code != 0 => {
                        return Outcome::Fail(format!(
                            "GHC disagrees with the expectation (exit {}):\n{}{}",
                            g.code,
                            cirka_testkit::line_diff(&expected, &g.stdout),
                            g.stderr
                        ));
                    }
                    // GHC is optional outside the Docker image.
                    Some(Err(e)) if !e.starts_with("cannot run") => return Outcome::Fail(e),
                    _ => {}
                }
            }
        }
        if programs == 0 {
            return Outcome::Fail("no Main.hs or Main.cirka".into());
        }
        Outcome::Pass
    });
}

/// Whole system headers: every declaration is read (no layout falls back
/// to an opaque type) and printed in both languages.
#[test]
fn bindgen_system_headers() {
    cirka_testkit::init();
    let cfg = cirka_driver::Config::from_env();
    let env = Env::from_env();
    let work = cirka_testkit::work_dir("bindgen-system", "headers");
    // (header, at least this many functions and records)
    let headers = [("math.h", 100, 0), ("vulkan/vulkan_core.h", 300, 500)];
    let mut failures = Vec::new();
    for (header, min_functions, min_records) in headers {
        if !includable(&cfg, header) {
            eprintln!("[bindgen-system] skipped <{header}>: not installed");
            continue;
        }
        let wrapper: PathBuf = work.join(format!("{}.h", header.replace('/', "_")));
        std::fs::write(&wrapper, format!("#include <{header}>\n")).unwrap();
        let opts = cirka_bindgen::Options {
            header: wrapper,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let h = match cirka_bindgen::generate(&env, &opts) {
            Ok(h) => h,
            Err(es) => {
                failures.push(format!("<{header}>:\n{}", es.join("\n")));
                continue;
            }
        };
        let hs = cirka_bindgen::print(&h, &opts, Lang::Haskell, header);
        let cirka = cirka_bindgen::print(&h, &opts, Lang::Cirka, header);
        let count = |f: fn(&Def) -> bool| h.items.iter().filter(|it| f(&it.def)).count();
        let functions = count(|d| matches!(d, Def::Function(_)));
        let records = count(|d| matches!(d, Def::Record(_)));
        eprintln!(
            "[bindgen-system] <{header}>: {} declarations ({functions} functions, {records} \
             records), {} + {} bytes, {:.2?}",
            h.items.len(),
            hs.len(),
            cirka.len(),
            start.elapsed()
        );
        for it in &h.items {
            if let Def::Opaque(Some(why)) = &it.def {
                failures.push(format!("<{header}>: {} is opaque: {why}", it.name));
            }
        }
        if functions < min_functions || records < min_records {
            failures.push(format!(
                "<{header}>: {functions} functions and {records} records (expected at least \
                 {min_functions} and {min_records})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
