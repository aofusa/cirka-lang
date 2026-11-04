//! The CirkaLang frontend of the shared driver: `.cirka` source files and
//! packages described by `cirka.toml` (FEAT-0205).
//!
//! Pipeline: parse + resolve (`cirkalang-resolve`) → elaborate and check
//! (`cirkalang-elab`, TT Lint in debug builds) → erase and lower to Core
//! (`cirkalang-lower`). Frontend dumps: `--dump=rn` (resolved program) and
//! `--dump=tt` (elaborated TT).

pub mod package;

use cirka_core::Program;
use cirka_diagnostics::{Diagnostic, SourceMap, Style, render};
use cirka_driver::pipeline::{Options, dump_frontend};
use std::path::{Path, PathBuf};

/// The version of the language implemented (`edition` in `cirka.toml`).
pub const EDITION: &str = "0.0.1";

/// Frontend stages accepted by `--dump=`.
pub const DUMPS: &[&str] = &["rn", "tt"];

/// Directory of the CirkaLang libraries: `CIRKA_LIB_DIR`, else
/// `../lib/cirkalang` next to the executable's directory (installed
/// layout), else the repository's `lib/cirkalang`.
pub fn lib_dir() -> PathBuf {
    if let Ok(d) = std::env::var("CIRKA_LIB_DIR")
        && !d.is_empty()
    {
        return PathBuf::from(d);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin) = exe.parent()
    {
        let cand = bin.join("../lib/cirkalang");
        if cand.join("core/Prelude.cirka").exists() {
            return cand;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib/cirkalang")
}

/// Source roots of the libraries.
pub fn lib_roots() -> Vec<PathBuf> {
    vec![lib_dir().join("core")]
}

/// The result of compiling a CirkaLang program.
pub struct Compiled {
    pub core: Program,
    /// Warnings (the program has no errors).
    pub warnings: Vec<Diagnostic>,
}

/// Compiles the program whose `main` module is `main_file` down to Core.
/// Diagnostics refer to files added to `sm`.
pub fn compile(
    sm: &mut SourceMap,
    main_file: &Path,
    roots: &[PathBuf],
    opts: &Options,
    name: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let rp = cirkalang_resolve::resolve_file(
        sm,
        main_file,
        roots,
        &cirkalang_resolve::Options::default(),
    )?;
    dump_frontend(
        opts,
        "rn",
        name,
        "rn",
        &cirkalang_resolve::print::print_program(&rp),
    )?;
    let el = cirkalang_elab::elaborate(&rp);
    let (errors, warnings): (Vec<Diagnostic>, Vec<Diagnostic>) =
        el.diags.into_iter().partition(|d| d.is_error());
    let Some(tt) = el.program else {
        let mut all = errors;
        all.extend(warnings);
        return Err(all);
    };
    dump_frontend(
        opts,
        "tt",
        name,
        "tt",
        &cirkalang_elab::tt::print_program(&tt),
    )?;
    if tt.main.is_none() {
        let root = rp
            .modules
            .last()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        return Err(vec![
            Diagnostic::error(format!("module `{root}` has no `main`"))
                .with_code("E1107")
                .with_help("define `main : IO ()`"),
        ]);
    }
    let core = cirkalang_lower::lower(&tt)?;
    Ok(Compiled { core, warnings })
}

/// The frontend callback of `cirka_driver::cli::main`.
pub fn frontend(
    sm: &mut SourceMap,
    path: &Path,
    opts: &Options,
) -> Option<Result<Program, Vec<Diagnostic>>> {
    if let Some(st) = opts
        .frontend_dumps
        .iter()
        .find(|s| *s != "all" && !DUMPS.contains(&s.as_str()))
    {
        return Some(Err(vec![Diagnostic::error(format!(
            "unknown dump stage `{st}` (frontend stages: {})",
            DUMPS.join(", ")
        ))]));
    }
    let (main_file, mut roots, name) =
        if path.is_dir() || path.file_name().is_some_and(|f| f == "cirka.toml") {
            match package::Package::load(path) {
                Ok(p) => (p.main_file(), vec![p.src_dir()], p.name),
                Err(e) => return Some(Err(vec![e])),
            }
        } else if path.extension().is_some_and(|e| e == "cirka") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
                .to_string();
            (path.to_path_buf(), Vec::new(), stem)
        } else {
            return None;
        };
    roots.extend(lib_roots());
    Some(compile(sm, &main_file, &roots, opts, &name).map(|c| {
        for w in &c.warnings {
            eprint!("{}", render(w, sm, Style::for_stderr()));
        }
        c.core
    }))
}
