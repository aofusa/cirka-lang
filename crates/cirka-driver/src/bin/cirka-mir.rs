//! Development command for hand-written MIR (FEAT-0014):
//!
//! ```text
//! cirka-mir check foo.mir             # parse + verify
//! cirka-mir interp foo.mir            # run on the reference interpreter
//! cirka-mir run [--debug-rt] [-O0..3] foo.mir [-- args]   # native
//! cirka-mir build [-O0..3] [-o out] foo.mir                 # native executable
//! cirka-mir emit-llvm foo.mir         # print LLVM IR
//! cirka-mir emit-mir foo.mir          # print the MIR (after --perceus)
//! ```
//!
//! `--perceus` runs the Perceus pipeline (borrow inference, dup/drop
//! insertion, reuse, fusion) on functions without reference counting first.

use cirka_diagnostics::{SourceMap, Style, render};
use cirka_driver::Config;
use cirka_driver::native::{NativeOptions, RuntimeKind, build_mir};
use std::process::ExitCode;

fn main() -> ExitCode {
    cirka_diagnostics::install_ice_hook("cirka-mir");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!(
                "usage: cirka-mir <check|interp|run|build|emit-llvm|emit-mir> [options] file.mir [-- args]"
            );
            return ExitCode::from(2);
        }
    };
    let mut opts = NativeOptions::default();
    let mut file = None;
    let mut prog_args = Vec::new();
    let mut output: Option<String> = None;
    let mut perceus = false;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--debug-rt" | "--runtime=debug" => opts.runtime = RuntimeKind::Debug,
            "--runtime=release" => opts.runtime = RuntimeKind::Release,
            "--runtime=tsan" => opts.runtime = RuntimeKind::Tsan,
            "--perceus" => perceus = true,
            s if s.starts_with("-O") => opts.opt_level = s[2..].parse().unwrap_or(2),
            "-o" => output = it.next().cloned(),
            "--" => {
                prog_args.extend(it.by_ref().cloned());
            }
            s => file = Some(s.to_string()),
        }
    }
    let Some(file) = file else {
        eprintln!("cirka-mir: no input file");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cirka-mir: cannot read {file}: {e}");
            return ExitCode::from(2);
        }
    };
    let mut sm = SourceMap::new();
    let fid = sm.add(file.clone(), text.clone());
    let parsed = if file.ends_with(".core") {
        // Common Core input: Lint, lower to MIR, then always run Perceus.
        perceus = true;
        cirka_core::parse_program(fid, &text)
            .map_err(|d| vec![d])
            .and_then(|p| {
                let errs = cirka_core::lint::lint_program(&p);
                if !errs.is_empty() {
                    return Err(errs);
                }
                cirka_mir::lower::lower_program(&p)
            })
    } else {
        cirka_mir::parse(fid, &text).map_err(|d| vec![d])
    };
    let mut m = match parsed {
        Ok(m) => m,
        Err(ds) => {
            for d in &ds {
                eprint!("{}", render(d, &sm, Style::for_stderr()));
            }
            return ExitCode::from(1);
        }
    };
    if perceus {
        cirka_mir::perceus::run(&mut m);
    }
    if std::env::var_os("CIRKA_DUMP_MIR").is_some() {
        eprint!("{}", cirka_mir::print_module(&m));
    }
    let errs = cirka_mir::verify::verify_module(&m);
    if !errs.is_empty() {
        for d in &errs {
            eprint!("{}", render(d, &sm, Style::for_stderr()));
        }
        return ExitCode::from(1);
    }
    let cfg = Config::from_env();
    match cmd {
        "check" => ExitCode::SUCCESS,
        "emit-mir" => {
            print!("{}", cirka_mir::print_module(&m));
            ExitCode::SUCCESS
        }
        "interp" => match cirka_mir::interp::run(&m, &[], u64::MAX) {
            Ok(r) => {
                print!("{}", r.stdout);
                eprint!("{}", r.stderr);
                eprintln!(
                    "cirka-mir: allocs={} frees={} reuses={} leaked={}",
                    r.stats.allocs, r.stats.frees, r.stats.reuses, r.leaked
                );
                ExitCode::from(r.exit_code as u8)
            }
            Err(e) => {
                eprintln!("cirka-mir: {e}");
                ExitCode::from(101)
            }
        },
        "emit-llvm" => {
            print!(
                "{}",
                cirka_codegen_llvm::emit(&m, opts.runtime.codegen_options())
            );
            ExitCode::SUCCESS
        }
        "build" => {
            let out = output.unwrap_or_else(|| "a.out".into());
            let work = std::env::temp_dir().join(format!("cirka-mir-{}", std::process::id()));
            let r = build_mir(&cfg, &m, &work, std::path::Path::new(&out), &opts);
            let _ = std::fs::remove_dir_all(&work);
            match r {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("cirka-mir: {e}");
                    ExitCode::from(1)
                }
            }
        }
        "run" => {
            let work = std::env::temp_dir().join(format!("cirka-mir-{}", std::process::id()));
            let exe = work.join("a.out");
            if let Err(e) = build_mir(&cfg, &m, &work, &exe, &opts) {
                eprintln!("cirka-mir: {e}");
                return ExitCode::from(1);
            }
            let st = std::process::Command::new(&exe).args(&prog_args).status();
            let _ = std::fs::remove_dir_all(&work);
            match st {
                Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
                Err(e) => {
                    eprintln!("cirka-mir: {e}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!("cirka-mir: unknown command `{cmd}`");
            ExitCode::from(2)
        }
    }
}
