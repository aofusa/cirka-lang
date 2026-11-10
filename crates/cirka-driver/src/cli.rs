//! Command-line handling shared by `achc` and `cirka` (01-architecture.md
//! §5): `build`, `run`, `check`, common flags and `.core` input.
//!
//! ```text
//! <tool> build [-o out] [flags] FILE
//! <tool> run [flags] FILE [-- args...]
//! <tool> check [flags] FILE
//! <tool> test [flags] [--filter=S] FILE   (frontends with tests in sources)
//! flags: -O0|-O1|-O2  --release (= -O2)  --runtime=release|debug
//!        --dump=core,core-opt,mir,mir-opt,llvm (+ frontend stages)  --build-dir=DIR
//!        --emit=llvm|exe  -l<lib> / -L<dir> (passed to the linker)
//!        --NAME=VALUE (other options, checked by the frontend)
//! ```

use crate::Config;
use crate::native::RuntimeKind;
use crate::pipeline::{self, Options, SourceTests, Stage};
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
    Test,
}

#[derive(Debug)]
pub struct Args {
    pub command: Command,
    pub file: PathBuf,
    pub output: Option<PathBuf>,
    pub prog_args: Vec<String>,
    pub emit_llvm: bool,
    pub opts: Options,
    /// `test --filter=S`: run only the tests whose name contains `S`.
    pub filter: Option<String>,
}

/// Parses the common arguments (after the tool name).
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let (cmd, rest) = args.split_first().ok_or("missing command")?;
    let command = match cmd.as_str() {
        "build" => Command::Build,
        "run" => Command::Run,
        "check" => Command::Check,
        "test" => Command::Test,
        c => return Err(format!("unknown command `{c}`")),
    };
    let mut file = None;
    let mut output = None;
    let mut prog_args = Vec::new();
    let mut emit_llvm = false;
    let mut opts = Options::new("out");
    let mut build_dir = None;
    let mut filter = None;
    let mut runtime_set = false;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-O0" => opts.opt_level = 0,
            "-O1" => opts.opt_level = 1,
            "-O2" | "-O3" | "--release" => opts.opt_level = 2,
            "--runtime=release" => {
                opts.runtime = RuntimeKind::Release;
                runtime_set = true;
            }
            "--runtime=debug" => {
                opts.runtime = RuntimeKind::Debug;
                runtime_set = true;
            }
            s if command == Command::Test && s.starts_with("--filter=") => {
                filter = Some(s["--filter=".len()..].to_string())
            }
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
            // Frontend options are checked by the frontend.
            s if s.starts_with("--") && s.contains('=') => opts.frontend_flags.push(s.to_string()),
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
    opts.tests = match command {
        Command::Check => SourceTests::Check,
        Command::Test => SourceTests::Run,
        _ => SourceTests::Strip,
    };
    // Tests check for leaks, which needs the debug runtime.
    if command == Command::Test && !runtime_set {
        opts.runtime = RuntimeKind::Debug;
    }
    Ok(Args {
        command,
        file,
        output,
        prog_args,
        emit_llvm,
        opts,
        filter,
    })
}

/// Stack of the compiling thread: the compilers recurse over deeply nested
/// terms (reserved lazily, so this costs nothing when unused).
pub const STACK_SIZE: usize = 256 << 20;

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
    if is_core && let Some(f) = args.opts.frontend_flags.first() {
        eprintln!("{tool}: unknown option `{f}`\n{usage}");
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
    if args.command == Command::Test {
        return run_tests(tool, &exe, args.filter.as_deref());
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

/// Runs the tests of a test executable (`exe --list`, then `exe N` for
/// each test in its own process, with the leak check on) and prints the
/// results like `cargo test`.
fn run_tests(tool: &str, exe: &Path, filter: Option<&str>) -> ExitCode {
    let exe = if exe.is_relative() {
        Path::new(".").join(exe)
    } else {
        exe.to_path_buf()
    };
    let list = match std::process::Command::new(&exe).arg("--list").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        Ok(o) => {
            eprintln!(
                "{tool}: cannot list the tests of {}\n{}",
                exe.display(),
                String::from_utf8_lossy(&o.stderr)
            );
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("{tool}: cannot run {}: {e}", exe.display());
            return ExitCode::from(1);
        }
    };
    let tests: Vec<(usize, &str, &str)> = list
        .lines()
        .enumerate()
        .filter_map(|(i, l)| l.split_once('\t').map(|(m, n)| (i, m, n)))
        .filter(|(_, _, n)| filter.is_none_or(|f| n.contains(f)))
        .collect();
    let total = list.lines().count();
    println!("running {} tests", tests.len());
    let mut failed = Vec::new();
    for (i, m, n) in &tests {
        let out = std::process::Command::new(&exe)
            .arg(i.to_string())
            .env("CIRKA_LEAK_CHECK", "1")
            .output();
        let detail = match &out {
            Ok(o) => test_failure(
                o.status.code(),
                &String::from_utf8_lossy(&o.stdout),
                &String::from_utf8_lossy(&o.stderr),
            ),
            Err(e) => Some(format!("cannot run: {e}\n")),
        };
        println!(
            "test {m} {n:?} ... {}",
            if detail.is_none() { "ok" } else { "FAILED" }
        );
        if let Some(detail) = detail {
            failed.push((format!("{m} {n:?}"), detail));
        }
    }
    if !failed.is_empty() {
        println!("\nfailures:");
        for (n, d) in &failed {
            print!("\n---- {n} ----\n{d}");
        }
    }
    let passed = tests.len() - failed.len();
    println!(
        "\ntest result: {}. {passed} passed; {} failed; {} filtered out",
        if failed.is_empty() { "ok" } else { "FAILED" },
        failed.len(),
        total - tests.len()
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Why a test run failed (its output), or `None` when it passed: it must
/// exit with 0 (`code` is `None` for a signal) and leak nothing.
fn test_failure(code: Option<i32>, stdout: &str, stderr: &str) -> Option<String> {
    let leaked = stderr.contains(" objects leaked");
    if code == Some(0) && !leaked {
        return None;
    }
    // Objects alive at a panic or `exit` are not leaks (05-runtime.md):
    // report them only for runs that otherwise passed.
    let stderr: String = if code == Some(0) {
        stderr.to_string()
    } else {
        stderr
            .lines()
            .filter(|l| !l.starts_with("cirka: leaked object") && !l.ends_with(" objects leaked"))
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let end = match code {
        Some(0) => String::new(),
        Some(c) => format!("exit code {c}\n"),
        None => "terminated by a signal\n".to_string(),
    };
    Some(format!("{stdout}{stderr}{end}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(&s.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn test_command_defaults_to_the_debug_runtime() {
        let a = args("test --filter=abc Main.cirka").unwrap();
        assert_eq!(a.command, Command::Test);
        assert_eq!(a.filter.as_deref(), Some("abc"));
        assert_eq!(a.opts.runtime, RuntimeKind::Debug);
        assert_eq!(a.opts.tests, SourceTests::Run);
        let a = args("test --runtime=release Main.cirka").unwrap();
        assert_eq!(a.opts.runtime, RuntimeKind::Release);
        assert_eq!(
            args("check Main.cirka").unwrap().opts.tests,
            SourceTests::Check
        );
        assert_eq!(
            args("build Main.cirka").unwrap().opts.tests,
            SourceTests::Strip
        );
        // `--filter=` is an option of `test` only.
        assert_eq!(
            args("build --filter=x Main.cirka")
                .unwrap()
                .opts
                .frontend_flags,
            vec!["--filter=x"]
        );
    }

    #[test]
    fn tests_fail_on_exit_codes_signals_and_leaks() {
        assert_eq!(test_failure(Some(0), "hidden\n", ""), None);
        let leak = "cirka: leaked object #9 kind=0 tag=0 size=16\ncirka: 1 objects leaked\n";
        // A leak fails a test that otherwise passed, and is shown.
        assert_eq!(test_failure(Some(0), "", leak).unwrap(), leak);
        // After a panic, live objects are not leaks and are not shown.
        assert_eq!(
            test_failure(Some(1), "", &format!("t: boom\n{leak}")).unwrap(),
            "t: boom\nexit code 1\n"
        );
        assert_eq!(
            test_failure(None, "out\n", "").unwrap(),
            "out\nterminated by a signal\n"
        );
    }

    #[test]
    fn frontend_flags_are_passed_through() {
        let a = args("check --ownership=off -O0 --ownership=strict Main.hs").unwrap();
        assert_eq!(a.command, Command::Check);
        assert_eq!(a.opts.opt_level, 0);
        assert_eq!(
            a.opts.frontend_flags,
            vec!["--ownership=off", "--ownership=strict"]
        );
        assert!(args("check --bogus Main.hs").is_err());
        assert!(args("check -x Main.hs").is_err());
    }
}
