//! The benchmark programs (FEAT-0043, 10-testing.md §5) print the same as
//! their Rust baseline for a small argument.
//!
//! Each `bench/<name>/check` holds the argument on its first line and the
//! expected output after it. Every implementation of the directory is
//! compiled at -O2 and run by the MIR interpreter without leaks:
//! `plain.hs` / `own.chs` with aCHC (unless the file says
//! `-- achc: skip`), `main.cirka` with CirkaLang. `rust/main.rs` must print
//! the expectation too, and `plain.hs` under GHC when `CIRKA_GHC` is set.

use cirka_diagnostics::{SourceMap, Style, render};
use cirka_testkit::repo_root;
use std::path::{Path, PathBuf};

/// An implementation compiled to Core, or the rendered errors.
fn compile(path: &Path) -> Result<cirka_core::Program, String> {
    let mut sm = SourceMap::new();
    let opts = cirka_driver::pipeline::Options::new("bench");
    let root = repo_root().display().to_string() + "/";
    let render_diags = |sm: &SourceMap, ds: &[cirka_diagnostics::Diagnostic]| {
        ds.iter()
            .map(|d| render(d, sm, Style::Plain))
            .collect::<String>()
            .replace(&root, "")
    };
    let dir = path.parent().unwrap().to_path_buf();
    if path.extension().is_some_and(|e| e == "cirka") {
        let mut roots = vec![dir];
        roots.extend(cirka::lib_roots());
        match cirka::compile(&mut sm, path, &roots, &opts, "Main") {
            Ok(c) if c.warnings.is_empty() => Ok(c.core),
            Ok(c) => Err(format!("warnings\n{}", render_diags(&sm, &c.warnings))),
            Err(ds) => Err(render_diags(&sm, &ds)),
        }
    } else {
        let roots = vec![dir, achc::lib_dir()];
        match achc::compile(&mut sm, path, &roots, &opts, "bench") {
            Ok(c) if c.warnings.is_empty() => Ok(c.core),
            Ok(c) => Err(format!("warnings\n{}", render_diags(&sm, &c.warnings))),
            Err(ds) => Err(render_diags(&sm, &ds)),
        }
    }
}

fn run_interp(name: &str, core: cirka_core::Program, arg: &str) -> Result<String, String> {
    let mut opts = cirka_driver::pipeline::Options::new(name);
    opts.opt_level = 2;
    opts.build_dir = cirka_testkit::work_dir("bench", name);
    let out = cirka_driver::pipeline::compile_core(core, name, &opts, false)
        .map_err(|ds| ds[0].message.clone())?;
    let r = cirka_mir::interp::run_with_args(&out.mir, &[arg], b"", u64::MAX)?;
    if r.exit_code != 0 || r.leaked != 0 {
        return Err(format!(
            "exit {} leaked {}\n{}{}",
            r.exit_code, r.leaked, r.stdout, r.stderr
        ));
    }
    Ok(r.stdout)
}

fn run_rust(name: &str, src: &Path, arg: &str) -> Result<String, String> {
    let exe = cirka_testkit::work_dir("bench", name).join("rust");
    let st = std::process::Command::new("rustc")
        .args(["-O", "-o"])
        .arg(&exe)
        .arg(src)
        .status()
        .map_err(|e| format!("cannot run rustc: {e}"))?;
    if !st.success() {
        return Err("rustc failed".into());
    }
    let out = std::process::Command::new(&exe)
        .arg(arg)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn skipped(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .take(5)
        .any(|l| l.trim() == "-- achc: skip")
}

#[test]
fn bench_programs() {
    cirka_testkit::init();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(repo_root().join("bench"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|d| d.join("check").exists())
        .collect();
    dirs.sort();
    assert!(dirs.len() >= 6, "bench programs with a `check` file");
    let mut failed = Vec::new();
    let mut ran = 0;
    std::thread::scope(|s| {
        let h = std::thread::Builder::new()
            .stack_size(cirka_testkit::COMPILER_STACK)
            .spawn_scoped(s, || {
                for dir in &dirs {
                    let bench = dir.file_name().unwrap().to_string_lossy().to_string();
                    let check = std::fs::read_to_string(dir.join("check")).unwrap();
                    let (arg, want) = check.split_once('\n').unwrap();
                    let rust = dir.join("rust/main.rs");
                    if rust.exists() {
                        match run_rust(&bench, &rust, arg) {
                            Ok(out) if out == want => {}
                            Ok(out) => failed.push(format!("{bench}/rust: printed\n{out}")),
                            // rustc is optional outside the Docker image.
                            Err(e) if e.starts_with("cannot run") => {}
                            Err(e) => failed.push(format!("{bench}/rust: {e}")),
                        }
                    }
                    let plain = dir.join("plain.hs");
                    if plain.exists()
                        && let Some(r) =
                            cirka_testkit::ghc_reference(&plain, None, &[arg.to_string()])
                    {
                        match r {
                            Ok(g) if g.stdout == want => {}
                            Ok(g) => failed.push(format!("{bench}/ghc: printed\n{}", g.stdout)),
                            Err(e) if e.starts_with("cannot run") => {}
                            Err(e) => failed.push(format!("{bench}/ghc: {e}")),
                        }
                    }
                    for file in ["plain.hs", "own.chs", "main.cirka"] {
                        let path = dir.join(file);
                        if !path.exists() || skipped(&path) {
                            continue;
                        }
                        let name = format!("{bench}-{}", file.replace('.', "-"));
                        ran += 1;
                        match compile(&path).and_then(|core| run_interp(&name, core, arg)) {
                            Ok(out) if out == want => {}
                            Ok(out) => failed.push(format!(
                                "{bench}/{file}: output differs\n{}",
                                cirka_testkit::line_diff(want, &out)
                            )),
                            Err(e) => failed.push(format!("{bench}/{file}: {e}")),
                        }
                    }
                }
            })
            .unwrap();
        h.join().unwrap();
    });
    assert!(failed.is_empty(), "{}", failed.join("\n"));
    assert!(ran >= 12, "only {ran} implementations ran");
}
