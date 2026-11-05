//! The shared pipeline from common Core to an executable (01-architecture.md
//! §1): Core Lint → Core optimizations → Core→MIR → Perceus → MIR verifier
//! → LLVM IR → clang + runtime. Intermediate results go to `_build/`.

use crate::Config;
use crate::native::{NativeOptions, RuntimeKind, link_ir};
use cirka_core::Program;
use cirka_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};

/// Stages that can be dumped with `--dump=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    Core,
    CoreOpt,
    Mir,
    MirOpt,
    Llvm,
}

impl Stage {
    pub const ALL: [Stage; 5] = [
        Stage::Core,
        Stage::CoreOpt,
        Stage::Mir,
        Stage::MirOpt,
        Stage::Llvm,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Stage::Core => "core",
            Stage::CoreOpt => "core-opt",
            Stage::Mir => "mir",
            Stage::MirOpt => "mir-opt",
            Stage::Llvm => "llvm",
        }
    }

    /// File extension of the dump.
    pub fn ext(self) -> &'static str {
        match self {
            Stage::Core => "core",
            Stage::CoreOpt => "opt.core",
            Stage::Mir => "mir",
            Stage::MirOpt => "opt.mir",
            Stage::Llvm => "ll",
        }
    }

    pub fn from_name(s: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|x| x.name() == s)
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    /// 0: no Core optimizations, clang -O0; 1 / 2: optimizations.
    pub opt_level: u8,
    pub runtime: RuntimeKind,
    pub dumps: Vec<Stage>,
    /// Dumps of frontend stages (e.g. `rn`, `tt`), checked by the frontend;
    /// `all` requests every stage.
    pub frontend_dumps: Vec<String>,
    /// Options of the form `--name=value` that the common driver does not
    /// know (e.g. aCHC's `--ownership=`), in order; checked by the frontend.
    pub frontend_flags: Vec<String>,
    /// Directory for intermediate files (default `_build/<name>`).
    pub build_dir: PathBuf,
    pub link_args: Vec<String>,
}

impl Options {
    pub fn new(name: &str) -> Options {
        Options {
            opt_level: 2,
            runtime: RuntimeKind::Release,
            dumps: Vec::new(),
            frontend_dumps: Vec::new(),
            frontend_flags: Vec::new(),
            build_dir: PathBuf::from("_build").join(name),
            link_args: Vec::new(),
        }
    }
}

/// Result of a successful compilation.
pub struct Output {
    pub mir: cirka_mir::Module,
    pub llvm: String,
}

fn to_ice(d: Diagnostic) -> Diagnostic {
    Diagnostic::ice(d.message)
}

fn dump(opts: &Options, stage: Stage, name: &str, text: &str) -> Result<(), Vec<Diagnostic>> {
    if !opts.dumps.contains(&stage) {
        return Ok(());
    }
    let p = opts.build_dir.join(format!("{name}.{}", stage.ext()));
    write_file(&p, text)
}

/// Writes the dump of a frontend stage to `<build_dir>/<name>.<ext>` when
/// it was requested with `--dump=<stage>` (or `--dump=all`).
pub fn dump_frontend(
    opts: &Options,
    stage: &str,
    name: &str,
    ext: &str,
    text: &str,
) -> Result<(), Vec<Diagnostic>> {
    if !opts.frontend_dumps.iter().any(|s| s == stage || s == "all") {
        return Ok(());
    }
    write_file(&opts.build_dir.join(format!("{name}.{ext}")), text)
}

fn write_file(p: &Path, text: &str) -> Result<(), Vec<Diagnostic>> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| {
            vec![Diagnostic::error(format!(
                "cannot create {}: {e}",
                d.display()
            ))]
        })?;
    }
    std::fs::write(p, text).map_err(|e| {
        vec![Diagnostic::error(format!(
            "cannot write {}: {e}",
            p.display()
        ))]
    })
}

/// Core → LLVM IR. `user_core` is true when the Core came from a `.core`
/// file (Lint errors are user errors); otherwise a Lint failure is an ICE.
pub fn compile_core(
    p: Program,
    name: &str,
    opts: &Options,
    user_core: bool,
) -> Result<Output, Vec<Diagnostic>> {
    let mut p = p;
    dump(opts, Stage::Core, name, &cirka_core::print_program(&p))?;
    let errs = cirka_core::lint::lint_program(&p);
    if !errs.is_empty() {
        if user_core {
            return Err(errs);
        }
        return Err(errs.into_iter().map(to_ice).collect());
    }
    if opts.opt_level > 0 {
        cirka_core::opt::optimize(&mut p).map_err(|e| vec![to_ice(e)])?;
    } else {
        // Even unoptimized builds drop the library code `main` never uses.
        cirka_core::opt::dce_globals(&mut p);
    }
    dump(opts, Stage::CoreOpt, name, &cirka_core::print_program(&p))?;
    let mut m = cirka_mir::lower::lower_program(&p)?;
    dump(opts, Stage::Mir, name, &cirka_mir::print_module(&m))?;
    cirka_mir::perceus::run(&mut m);
    dump(opts, Stage::MirOpt, name, &cirka_mir::print_module(&m))?;
    let errs = cirka_mir::verify::verify_module(&m);
    if !errs.is_empty() {
        return Err(errs.into_iter().map(to_ice).collect());
    }
    let llvm = cirka_codegen_llvm::emit(
        &m,
        cirka_codegen_llvm::Options {
            debug_rt: opts.runtime == RuntimeKind::Debug,
        },
    );
    dump(opts, Stage::Llvm, name, &llvm)?;
    Ok(Output { mir: m, llvm })
}

/// Links the LLVM IR of `out` into `exe`.
pub fn link(cfg: &Config, out: &Output, opts: &Options, exe: &Path) -> Result<(), Vec<Diagnostic>> {
    let nopts = NativeOptions {
        opt_level: opts.opt_level.min(3),
        runtime: opts.runtime,
        link_args: opts.link_args.clone(),
    };
    link_ir(cfg, &out.llvm, &opts.build_dir, exe, &nopts).map_err(|e| vec![Diagnostic::error(e)])
}
