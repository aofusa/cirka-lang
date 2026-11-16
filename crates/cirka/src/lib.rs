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
use cirka_driver::pipeline::{Options, SourceTests, dump_frontend, timed};
use cirkalang_resolve::TestMode;
use cirkalang_resolve::rast;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The version of the language implemented (`edition` in `cirka.toml`).
pub const EDITION: &str = "0.0.5";

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
    // `vulkan`: `Vulkan` and the bindings `Vulkan.Sys` (spec-v0.0.6.md §3);
    // `linalg`: `Linalg` on OpenBLAS and LAPACKE (§4.2).
    vec![
        lib_dir().join("core"),
        lib_dir().join("std"),
        lib_dir().join("vulkan"),
        lib_dir().join("linalg"),
    ]
}

/// The result of compiling a CirkaLang program.
pub struct Compiled {
    pub core: Program,
    /// Warnings (the program has no errors).
    pub warnings: Vec<Diagnostic>,
}

fn resolve_options(opts: &Options) -> cirkalang_resolve::Options {
    cirkalang_resolve::Options {
        tests: match opts.tests {
            SourceTests::Strip => TestMode::Strip,
            SourceTests::Check => TestMode::Check,
            SourceTests::Run => TestMode::Run,
        },
        ..Default::default()
    }
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
    let rp = timed(opts, "parse and resolve", || {
        cirkalang_resolve::resolve_file(sm, main_file, roots, &resolve_options(opts))
    })?;
    let libs = library_modules(sm, &rp);
    compile_resolved(rp, &libs, opts, name)
}

/// The modules read from the libraries of the compiler (`lib_dir()`): the
/// core library and the standard library.
pub fn library_modules(sm: &SourceMap, rp: &rast::Program) -> HashSet<String> {
    let Ok(lib) = std::fs::canonicalize(lib_dir()) else {
        return HashSet::new();
    };
    rp.modules
        .iter()
        .filter(|m| std::fs::canonicalize(&sm.file(m.file).name).is_ok_and(|p| p.starts_with(&lib)))
        .map(|m| m.name.clone())
        .collect()
}

/// Declarations that only the libraries of the compiler may contain
/// (spec-v0.0.1.md §5.6, BUG-0023): `prim`, whose implementation the
/// compiler gives, and `trusted` (spec-v0.0.5.md §4.2), whose result is
/// assumed. Some primitives rely on what the library proved before
/// calling them (`primArrayIndexProved` reads without a bounds check), so
/// a program declaring them, or assuming a false size, could break memory
/// safety.
fn library_only(rp: &rast::Program, libs: &HashSet<String>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for m in rp.modules.iter().filter(|m| !libs.contains(&m.name)) {
        for it in &m.items {
            match it {
                rast::Item::Prim(g, _) => out.push(
                    Diagnostic::error(format!(
                        "the primitive `{}` is declared outside the libraries",
                        rp.info(*g).name
                    ))
                    .with_code("E1009")
                    .with_label(
                        rp.info(*g).span,
                        "only `lib/cirkalang` may declare primitives",
                    )
                    .with_help("declare a function of another language in an `extern` block"),
                ),
                rast::Item::Def(d) if d.trusted.is_some() => out.push(
                    Diagnostic::error(format!(
                        "`{}` is trusted outside the libraries",
                        rp.info(d.name).name
                    ))
                    .with_code("E1009")
                    .with_label(
                        d.trusted.unwrap_or(d.span),
                        "only `lib/cirkalang` may assume what it does not prove",
                    )
                    .with_help("prove the contract, or check it at run time (`--contracts=check`)"),
                ),
                _ => {}
            }
        }
    }
    out
}

/// Compiles the package at `path` (a directory or its `cirka.toml`) and its
/// dependencies down to Core (FEAT-0214).
pub fn compile_package(
    sm: &mut SourceMap,
    path: &Path,
    opts: &Options,
) -> Result<Compiled, Vec<Diagnostic>> {
    let w = package::Workspace::load(path).map_err(|e| vec![e])?;
    let mut loader = w.loader(lib_roots());
    let main = w.root.main_file();
    if !main.exists() && opts.tests == SourceTests::Strip {
        return Err(vec![
            Diagnostic::error(format!(
                "package `{}` is a library: it has no `src/Main.cirka` to run",
                w.root.name
            ))
            .with_code("E1107")
            .with_help("check it with `cirka check` or run its tests with `cirka test`"),
        ]);
    }
    let rp = if main.exists() {
        timed(opts, "parse and resolve", || {
            cirkalang_resolve::resolve_main(sm, &main, &mut loader, &resolve_options(opts))
        })?
    } else {
        // A library: all its modules are checked and tested; there is
        // nothing to run (E1107 for `build` and `run`).
        let mods = w.root.modules();
        let text: String = mods.iter().map(|m| format!("import {m}\n")).collect();
        let id = sm.add(
            format!("{} (library)", w.root.src_dir().display()),
            text.clone(),
        );
        timed(opts, "parse and resolve", || {
            cirkalang_resolve::resolve_program(sm, id, &text, &mut loader, &resolve_options(opts))
        })?
    };
    let libs = library_modules(sm, &rp);
    compile_resolved(rp, &libs, opts, &w.root.name)
}

/// The frontend options.
struct Flags {
    /// `--contracts=off|check|verify` (spec-v0.0.4.md §4.3).
    contracts: cirkalang_check::Contracts,
    /// `--libraries=verify` (spec-v0.0.5.md §4.3): the proof obligations of
    /// the libraries are checked too (`--libraries=trust`, the default:
    /// their own tests verify them).
    verify_libraries: bool,
    /// `--extern-items=all` (spec-v0.0.6.md §1.10): the items of extern
    /// blocks that the program does not use are checked too
    /// (`--extern-items=used`, the default: only those it uses).
    every_extern_item: bool,
}

/// Parses the frontend options; others are unknown.
fn frontend_flags(opts: &Options) -> Result<Flags, Diagnostic> {
    let mut flags = Flags {
        contracts: cirkalang_check::Contracts::default(),
        verify_libraries: false,
        every_extern_item: false,
    };
    for f in &opts.frontend_flags {
        if let Some(v) = f.strip_prefix("--extern-items=") {
            flags.every_extern_item = match v {
                "all" => true,
                "used" => false,
                _ => {
                    return Err(
                        Diagnostic::error(format!("unknown extern items mode in `{f}`"))
                            .with_help("use `--extern-items=used` or `--extern-items=all`"),
                    );
                }
            };
            continue;
        }
        if let Some(v) = f.strip_prefix("--libraries=") {
            flags.verify_libraries = match v {
                "verify" => true,
                "trust" => false,
                _ => {
                    return Err(
                        Diagnostic::error(format!("unknown libraries mode in `{f}`"))
                            .with_help("use `--libraries=trust` or `--libraries=verify`"),
                    );
                }
            };
            continue;
        }
        let parsed = f
            .strip_prefix("--contracts=")
            .and_then(cirkalang_check::Contracts::parse);
        match parsed {
            Some(m) => flags.contracts = m,
            None if f.starts_with("--contracts=") => {
                return Err(
                    Diagnostic::error(format!("unknown contracts mode in `{f}`")).with_help(
                        "use `--contracts=verify`, `--contracts=check` or `--contracts=off`",
                    ),
                );
            }
            None => return Err(Diagnostic::error(format!("unknown option `{f}`"))),
        }
    }
    Ok(flags)
}

fn compile_resolved(
    rp: rast::Program,
    libs: &HashSet<String>,
    opts: &Options,
    name: &str,
) -> Result<Compiled, Vec<Diagnostic>> {
    let errs = library_only(&rp, libs);
    if !errs.is_empty() {
        return Err(errs);
    }
    dump_frontend(opts, "rn", name, "rn", || {
        cirkalang_resolve::print::print_program(&rp)
    })?;
    let flags = frontend_flags(opts).map_err(|d| vec![d])?;
    let el = timed(opts, "elaborate", || {
        cirkalang_elab::elaborate_with(&rp, flags.every_extern_item)
    });
    let (errors, warnings): (Vec<Diagnostic>, Vec<Diagnostic>) =
        el.diags.into_iter().partition(|d| d.is_error());
    let Some(mut tt) = el.program else {
        let mut all = errors;
        all.extend(warnings);
        return Err(all);
    };
    dump_frontend(opts, "tt", name, "tt", || {
        cirkalang_elab::tt::print_program(&tt)
    })?;
    // Totality and refinements (spec-v0.0.4.md §1, §3, §4). The libraries'
    // obligations are verified by their own tests (spec-v0.0.5.md §4.3).
    let mode = flags.contracts;
    let verified = if flags.verify_libraries {
        HashSet::new()
    } else {
        libs.clone()
    };
    let checked = timed(opts, "totality and refinements", || {
        cirkalang_check::check_with(
            &rp,
            &tt,
            Some(opts.build_dir.join("verify")),
            mode,
            &verified,
        )
    });
    let (errors, more): (Vec<Diagnostic>, Vec<Diagnostic>) =
        checked.diags.into_iter().partition(|d| d.is_error());
    let mut warnings = warnings;
    warnings.extend(more);
    if !errors.is_empty() {
        let mut all = errors;
        all.extend(warnings);
        return Err(all);
    }
    if !checked.unproved.is_empty() {
        cirkalang_check::insert_checks(&mut tt, &checked.unproved);
    }
    // Every refinement is now proved or checked at run time (spec-v0.0.4.md
    // §5); with `off`, nothing is.
    tt.proofs_trusted = mode != cirkalang_check::Contracts::Off;
    // A library (no `main`) can be checked: its Core is still linted.
    if tt.main.is_none() && opts.tests != SourceTests::Check {
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
    let core = timed(opts, "lower to Core", || cirkalang_lower::lower(&tt))?;
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
    if let Err(d) = frontend_flags(opts) {
        return Some(Err(vec![d]));
    }
    let compiled = if path.is_dir() || path.file_name().is_some_and(|f| f == "cirka.toml") {
        compile_package(sm, path, opts)
    } else if path.extension().is_some_and(|e| e == "cirka") {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("out")
            .to_string();
        compile(sm, path, &lib_roots(), opts, &stem)
    } else {
        return None;
    };
    Some(compiled.map(|c| {
        for w in &c.warnings {
            eprint!("{}", render(w, sm, Style::for_stderr()));
        }
        c.core
    }))
}
