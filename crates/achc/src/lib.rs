//! The Haskell frontend of the shared driver (aCHC): `.hs` / `.chs` source
//! files and packages described by `achc.toml` (FEAT-0105).
//!
//! Pipeline: parse + rename (`achc-rename`, which loads the imported
//! modules) → type inference (`achc-typeck`) → desugaring and match
//! compilation into Core (`achc-desugar`). Frontend dumps: `--dump=rn`
//! (renamed main module) and `--dump=tc` (typed program).

pub mod package;

use cirka_core::Program;
use cirka_diagnostics::{Diagnostic, SourceMap, Style, render};
use cirka_driver::pipeline::{Options, dump_frontend};
use std::path::{Path, PathBuf};

/// Frontend stages accepted by `--dump=`.
pub const DUMPS: &[&str] = &["rn", "tc"];

/// What `--cycles=` does with recursive value bindings (04-memory-model.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cycles {
    /// Warn (W0401); the cycles leak.
    Warn,
    /// Reject the program (W0401 as an error).
    Error,
    /// Collect the cycles at run time (trial deletion); no warning.
    Collect,
}

/// The frontend options (`--name=value`, the last one wins).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FrontendOptions {
    /// The ownership level of `.hs` files (default `warn`, 02-language-extensions.md §1.3).
    pub ownership: achc_typeck::Ownership,
    /// Default `warn`.
    pub cycles: Cycles,
}

/// Parses the frontend options: `--ownership=off|warn|strict` and
/// `--cycles=warn|error|collect`.
pub fn frontend_options(opts: &Options) -> Result<FrontendOptions, Diagnostic> {
    let mut o = FrontendOptions {
        ownership: achc_typeck::Ownership::Warn,
        cycles: Cycles::Warn,
    };
    let bad = |name: &str, v: &str, expected: &str| {
        Diagnostic::error(format!(
            "invalid value `{v}` for `--{name}` (expected {expected})"
        ))
    };
    for f in &opts.frontend_flags {
        if let Some(v) = f.strip_prefix("--ownership=") {
            o.ownership = achc_typeck::Ownership::parse(v)
                .ok_or_else(|| bad("ownership", v, "off, warn or strict"))?;
        } else if let Some(v) = f.strip_prefix("--cycles=") {
            o.cycles = match v {
                "warn" => Cycles::Warn,
                "error" => Cycles::Error,
                "collect" => Cycles::Collect,
                _ => return Err(bad("cycles", v, "warn, error or collect")),
            };
        } else {
            return Err(Diagnostic::error(format!("unknown option `{f}`")));
        }
    }
    Ok(o)
}

/// The ownership level given by the frontend options.
pub fn ownership_level(opts: &Options) -> Result<achc_typeck::Ownership, Diagnostic> {
    frontend_options(opts).map(|o| o.ownership)
}

/// Directory of the base library: `ACHC_LIB_DIR`, else
/// `../lib/cirka-base` next to the executable's directory (installed
/// layout), else the repository's `lib/cirka-base`.
pub fn lib_dir() -> PathBuf {
    if let Ok(d) = std::env::var("ACHC_LIB_DIR")
        && !d.is_empty()
    {
        return PathBuf::from(d);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin) = exe.parent()
    {
        let cand = bin.join("../lib/cirka-base");
        if cand.join("Prelude.hs").exists() {
            return cand;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lib/cirka-base")
}

/// The result of compiling a Haskell program.
pub struct Compiled {
    pub core: Program,
    /// Warnings (the program has no errors).
    pub warnings: Vec<Diagnostic>,
}

/// Compiles the program whose main module is `main_file` down to Core,
/// looking up imported modules in `roots` (in order). Diagnostics refer
/// to files added to `sm`.
pub fn compile(
    sm: &mut SourceMap,
    main_file: &Path,
    roots: &[PathBuf],
    opts: &Options,
    name: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let src = std::fs::read_to_string(main_file).map_err(|e| {
        vec![Diagnostic::error(format!(
            "cannot read {}: {e}",
            main_file.display()
        ))]
    })?;
    let fopts = frontend_options(opts).map_err(|d| vec![d])?;
    let ownership = fopts.ownership;
    let chs = main_file.extension().is_some_and(|e| e == "chs");
    let id = sm.add(main_file.display().to_string(), src.clone());
    let mut loader = achc_rename::FsLoader {
        roots: roots.to_vec(),
    };
    let rn = achc_rename::rename_program(
        sm,
        id,
        &src,
        chs,
        &mut loader,
        &achc_rename::Options::default(),
    )?;
    if let Some(m) = rn.modules.last() {
        dump_frontend(
            opts,
            "rn",
            name,
            "rn",
            &achc_syntax::print::print_module(&m.ast),
        )?;
    }
    let lib = lib_dir();
    let tp = achc_typeck::typecheck_in(
        &rn,
        Some(sm),
        &|f| std::path::Path::new(f).starts_with(&lib),
        ownership,
    )?;
    dump_frontend(
        opts,
        "tc",
        name,
        "tc",
        &achc_typeck::tast::print_program(&tp),
    )?;
    if tp.main.is_none() {
        let root = rn
            .modules
            .last()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        return Err(vec![
            Diagnostic::error(format!("module `{root}` has no `main`"))
                .with_code("E0606")
                .with_help("define `main :: IO ()`"),
        ]);
    }
    let mut out = achc_desugar::desugar(&tp)?;
    // Recursive value bindings (W0401) under `--cycles=`.
    let is_w0401 = |d: &Diagnostic| d.code.as_deref() == Some("W0401");
    match fopts.cycles {
        Cycles::Warn => {}
        Cycles::Collect => out.warnings.retain(|d| !is_w0401(d)),
        Cycles::Error => {
            let errs: Vec<Diagnostic> = out
                .warnings
                .iter()
                .filter(|d| is_w0401(d))
                .map(|d| {
                    let mut d = d.clone();
                    d.severity = cirka_diagnostics::Severity::Error;
                    d.with_note("an error under `--cycles=error`")
                })
                .collect();
            if !errs.is_empty() {
                return Err(errs);
            }
        }
    }
    out.program.collect_cycles = fopts.cycles == Cycles::Collect;
    Ok(Compiled {
        core: out.program,
        warnings: out.warnings,
    })
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
    let mut pkg_opts = None;
    let (main_file, mut roots, name) =
        if path.is_dir() || path.file_name().is_some_and(|f| f == "achc.toml") {
            match package::Package::load(path) {
                Ok(p) => {
                    // `--ownership=` / `--cycles=` on the command line override achc.toml.
                    let mut o = opts.clone();
                    o.frontend_flags
                        .insert(0, format!("--ownership={}", p.ownership));
                    o.frontend_flags.insert(1, format!("--cycles={}", p.cycles));
                    pkg_opts = Some(o);
                    (p.main_file(), p.source_dirs(), p.name)
                }
                Err(e) => return Some(Err(vec![e])),
            }
        } else if path.extension().is_some_and(|e| e == "hs" || e == "chs") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
                .to_string();
            let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
            (path.to_path_buf(), vec![dir], stem)
        } else {
            return None;
        };
    roots.push(lib_dir());
    let opts = pkg_opts.as_ref().unwrap_or(opts);
    Some(compile(sm, &main_file, &roots, opts, &name).map(|c| {
        for w in &c.warnings {
            eprint!("{}", render(w, sm, Style::for_stderr()));
        }
        c.core
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(flags: &[&str]) -> Result<achc_typeck::Ownership, Diagnostic> {
        let mut o = Options::new("t");
        o.frontend_flags = flags.iter().map(|s| s.to_string()).collect();
        ownership_level(&o)
    }

    #[test]
    fn ownership_flags() {
        use achc_typeck::Ownership::*;
        assert_eq!(level(&[]).unwrap(), Warn);
        assert_eq!(level(&["--ownership=off"]).unwrap(), Off);
        assert_eq!(
            level(&["--ownership=off", "--ownership=strict"]).unwrap(),
            Strict
        );
        assert!(level(&["--ownership=loose"]).is_err());
        assert!(level(&["--cycles=bad"]).is_err());
        assert!(level(&["--other=1"]).is_err());
        let mut o = Options::new("t");
        o.frontend_flags = vec!["--cycles=collect".into(), "--ownership=off".into()];
        assert_eq!(
            frontend_options(&o).unwrap(),
            FrontendOptions {
                ownership: Off,
                cycles: Cycles::Collect
            }
        );
    }
}
