//! Command-line handling shared by `achc` and `cirka` (01-architecture.md
//! §5): `build`, `run`, `check`, common flags and `.core` input.
//!
//! ```text
//! <tool> build [-o out] [flags] FILE
//! <tool> run [flags] FILE [-- args...]
//! <tool> check [flags] FILE
//! flags: -O0|-O1|-O2  --release (= -O2)  --runtime=release|debug
//!        --dump=core,core-opt,mir,mir-opt,llvm (+ frontend stages)  --build-dir=DIR
//!        --emit=llvm|exe  -l<lib> / -L<dir> (passed to the linker)
//! ```

use crate::Config;
use crate::native::RuntimeKind;
use crate::pipeline::{self, Options, Stage};
use cirka_core::Program;
use cirka_diagnostics::{Diagnostic, SourceMap, Style, render};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A frontend turns a source file into Core. It returns `None` when it does
/// not handle the file's extension.
pub type Frontend<'a> = &'a (
        dyn Fn(&mut SourceMap, &Path, &Options) -> Option<Result<Program, Vec<Diagnostic>>> + Sync
    );

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Build,
    Run,
    Check,
}

#[derive(Debug)]
pub struct Args {
    pub command: Command,
    pub file: PathBuf,
    pub output: Option<PathBuf>,
    pub prog_args: Vec<String>,
    pub emit_llvm: bool,
    pub opts: Options,
}

/// Parses the common arguments (after the tool name).
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let (cmd, rest) = args.split_first().ok_or("missing command")?;
    let command = match cmd.as_str() {
        "build" => Command::Build,
        "run" => Command::Run,
        "check" => Command::Check,
        c => return Err(format!("unknown command `{c}`")),
    };
    let mut file = None;
    let mut output = None;
    let mut prog_args = Vec::new();
    let mut emit_llvm = false;
    let mut opts = Options::new("out");
    let mut build_dir = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-O0" => opts.opt_level = 0,
            "-O1" => opts.opt_level = 1,
            "-O2" | "-O3" | "--release" => opts.opt_level = 2,
            "--runtime=release" => opts.runtime = RuntimeKind::Release,
            "--runtime=debug" => opts.runtime = RuntimeKind::Debug,
            "--emit=llvm" => emit_llvm = true,
            "--emit=exe" => emit_llvm = false,
            "-o" => output = Some(PathBuf::from(it.next().ok_or("`-o` needs a file name")?)),
            "--" => prog_args.extend(it.by_ref().cloned()),
            s if s.starts_with("--dump=") => {
                for st in s["--dump=".len()..].split(',') {
                    if st == "all" {
                        opts.dumps.extend(Stage::ALL);
                        opts.frontend_dumps.push(st.to_string());
                        continue;
                    }
                    match Stage::from_name(st) {
                        Some(x) => opts.dumps.push(x),
                        // Frontend stages are checked by the frontend.
                        None => opts.frontend_dumps.push(st.to_string()),
                    }
                }
            }
            s if s.starts_with("--build-dir=") => {
                build_dir = Some(PathBuf::from(&s["--build-dir=".len()..]))
            }
            s if s.starts_with("-l") || s.starts_with("-L") => opts.link_args.push(s.to_string()),
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`")),
            s => {
                if file.is_some() {
                    return Err(format!("unexpected argument `{s}`"));
                }
                file = Some(PathBuf::from(s));
            }
        }
    }
    let file = file.ok_or("no input file")?;
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out")
        .to_string();
    opts.build_dir = build_dir.unwrap_or_else(|| PathBuf::from("_build").join(&stem));
    Ok(Args {
        command,
        file,
        output,
        prog_args,
        emit_llvm,
        opts,
    })
}

/// Stack of the compiling thread: the compilers recurse over deeply nested
/// terms (reserved lazily, so this costs nothing when unused).
const STACK_SIZE: usize = 256 << 20;

/// Entry point of a CLI: `tool` is the program name used in messages.
pub fn main(tool: &'static str, usage: &str, frontend: Frontend) -> ExitCode {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn_scoped(s, || main_inner(tool, usage, frontend))
            .map(|h| h.join().unwrap_or(ExitCode::from(101)))
            .unwrap_or_else(|_| main_inner(tool, usage, frontend))
    })
}

fn main_inner(tool: &'static str, usage: &str, frontend: Frontend) -> ExitCode {
    cirka_diagnostics::install_ice_hook(tool);
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{tool}: {e}\n{usage}");
            return ExitCode::from(2);
        }
    };
    let mut sm = SourceMap::new();
    let report = |sm: &SourceMap, ds: &[Diagnostic]| {
        for d in ds {
            eprint!("{}", render(d, sm, Style::for_stderr()));
        }
    };
    let stem = args
        .file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out")
        .to_string();
    let is_core = args.file.extension().is_some_and(|e| e == "core");
    if is_core && let Some(st) = args.opts.frontend_dumps.iter().find(|s| *s != "all") {
        eprintln!("{tool}: unknown dump stage `{st}`\n{usage}");
        return ExitCode::from(2);
    }
    let program = if is_core {
        load_core(&mut sm, &args.file)
    } else {
        match frontend(&mut sm, &args.file, &args.opts) {
            Some(r) => r,
            None => {
                eprintln!("{tool}: unsupported input file `{}`", args.file.display());
                return ExitCode::from(2);
            }
        }
    };
    let program = match program {
        Ok(p) => p,
        Err(ds) => {
            report(&sm, &ds);
            return ExitCode::from(1);
        }
    };
    let out = match pipeline::compile_core(program, &stem, &args.opts, is_core) {
        Ok(o) => o,
        Err(ds) => {
            report(&sm, &ds);
            return ExitCode::from(1);
        }
    };
    if args.command == Command::Check {
        return ExitCode::SUCCESS;
    }
    if args.emit_llvm {
        let p = args
            .output
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("{stem}.ll")));
        if let Err(e) = std::fs::write(&p, &out.llvm) {
            eprintln!("{tool}: cannot write {}: {e}", p.display());
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }
    let cfg = Config::from_env();
    let exe = match (&args.output, args.command) {
        (Some(o), _) => o.clone(),
        (None, Command::Build) => PathBuf::from(&stem),
        (None, _) => args.opts.build_dir.join(&stem),
    };
    if let Err(ds) = pipeline::link(&cfg, &out, &args.opts, &exe) {
        report(&sm, &ds);
        return ExitCode::from(1);
    }
    if args.command == Command::Build {
        return ExitCode::SUCCESS;
    }
    let exe = if exe.is_relative() && exe.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        Path::new(".").join(&exe)
    } else {
        exe
    };
    match std::process::Command::new(&exe)
        .args(&args.prog_args)
        .status()
    {
        Ok(st) => match st.code() {
            Some(c) => ExitCode::from(c as u8),
            None => ExitCode::from(134),
        },
        Err(e) => {
            eprintln!("{tool}: cannot run {}: {e}", exe.display());
            ExitCode::from(1)
        }
    }
}

fn load_core(sm: &mut SourceMap, path: &Path) -> Result<Program, Vec<Diagnostic>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        vec![Diagnostic::error(format!(
            "cannot read {}: {e}",
            path.display()
        ))]
    })?;
    let fid = sm.add(path.display().to_string(), text.clone());
    cirka_core::parse_program(fid, &text).map_err(|d| vec![d])
}
