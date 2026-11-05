//! Compatibility measurement (FEAT-0119, 10-testing.md §4): programs from
//! GHC's testsuite and nofib under `tests/achc/compat/` are compiled with
//! aCHC, built natively and run; each result gets one category. The pass
//! rate is not a goal (ADR-025): the categories are the material of
//! `docs/design/compat-report.md`.
//!
//! The measurement is slow and runs only through `scripts/compat.sh`
//! (`--ignored`). It writes `docs/artifacts/compat/results.tsv` and compares
//! with `tests/achc/compat/baseline.tsv`: a program that passed there and
//! fails now is a regression (the test fails); `CIRKA_BLESS=1` rewrites the
//! baseline.

use cirka_diagnostics::{SourceMap, Style, render};
use cirka_testkit::{discover, repo_root};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Why a program does not behave like with GHC (or `Pass`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Category {
    Pass,
    /// Lexical or syntax error (E01xx).
    Parse,
    /// A language extension or construct aCHC does not support (E0001).
    Extension,
    /// A module that `cirka-base` does not provide.
    Module,
    /// A name missing from `cirka-base` (or an import item, E0201/E0205).
    Name,
    /// Type or class error (E06xx) of a program GHC accepts.
    Type,
    /// Ownership / linearity diagnostics (E03xx).
    Ownership,
    /// Other compile-time diagnostics (FFI, packages).
    OtherCompile,
    /// Recursive value bindings (cyclic data such as `let xs = 1 : xs`,
    /// W0401): runs correctly with the cycle collector (`--cycles=collect`,
    /// FEAT-0126), which reference counting alone cannot do.
    Cyclic,
    /// The compiler panicked or a later stage failed (internal error).
    Ice,
    /// Wrong exit code, runtime error or signal.
    Crash,
    /// Different standard output.
    Output,
    /// Correct output, but objects were leaked (RC cycles).
    Leak,
    /// No result within the time limit.
    Timeout,
}

impl Category {
    fn name(self) -> &'static str {
        match self {
            Category::Pass => "pass",
            Category::Parse => "parse",
            Category::Extension => "extension",
            Category::Module => "module",
            Category::Name => "name",
            Category::Type => "type",
            Category::Ownership => "ownership",
            Category::OtherCompile => "other-compile",
            Category::Cyclic => "cyclic",
            Category::Ice => "ice",
            Category::Crash => "crash",
            Category::Output => "output",
            Category::Leak => "leak",
            Category::Timeout => "timeout",
        }
    }

    fn all() -> [Category; 14] {
        [
            Category::Pass,
            Category::Parse,
            Category::Extension,
            Category::Module,
            Category::Name,
            Category::Type,
            Category::Ownership,
            Category::OtherCompile,
            Category::Cyclic,
            Category::Ice,
            Category::Crash,
            Category::Output,
            Category::Leak,
            Category::Timeout,
        ]
    }
}

/// The category of a program rejected by the front end, from its first
/// diagnostic.
fn classify_diag(code: &str, message: &str) -> Category {
    match code {
        "E0001" => Category::Extension,
        // Foreign imports and primitives.
        "E0607" | "E0618" => Category::OtherCompile,
        "E0201" | "E0205" if message.starts_with("module ") => Category::Module,
        "E0201" | "E0205" => Category::Name,
        c if c.starts_with("E01") => Category::Parse,
        c if c.starts_with("E02") => Category::Name,
        c if c.starts_with("E06") => Category::Type,
        c if c.starts_with("E03") => Category::Ownership,
        _ => Category::OtherCompile,
    }
}

/// The category of a run whose program compiled.
fn classify_run(
    want_out: &str,
    want_code: i32,
    out: &str,
    code: Option<i32>,
    stderr: &str,
) -> Category {
    let Some(code) = code else {
        return Category::Timeout;
    };
    if code != want_code {
        return Category::Crash;
    }
    if out != want_out {
        return Category::Output;
    }
    if stderr.contains("objects leaked") {
        return Category::Leak;
    }
    Category::Pass
}

#[test]
fn compat_classification() {
    assert_eq!(classify_diag("E0102", "expected `)`"), Category::Parse);
    assert_eq!(
        classify_diag("E0201", "module `Data.Map` not found"),
        Category::Module
    );
    assert_eq!(
        classify_diag("E0201", "`foldl'` is not defined"),
        Category::Name
    );
    assert_eq!(
        classify_diag("E0001", "multi-parameter type classes are not supported"),
        Category::Extension
    );
    assert_eq!(
        classify_diag("E0608", "no instance for `Show T`"),
        Category::Type
    );
    assert_eq!(classify_diag("E0618", "ffi"), Category::OtherCompile);
    assert_eq!(classify_run("a\n", 0, "a\n", Some(0), ""), Category::Pass);
    assert_eq!(classify_run("a\n", 0, "b\n", Some(0), ""), Category::Output);
    assert_eq!(classify_run("a\n", 0, "a\n", Some(1), ""), Category::Crash);
    assert_eq!(classify_run("a\n", 0, "", None, ""), Category::Timeout);
    assert_eq!(
        classify_run("a\n", 0, "a\n", Some(0), "cirka: 3 objects leaked\n"),
        Category::Leak
    );
}

struct Program {
    /// `ghc/<dir>/<name>` or `nofib/<name>`.
    name: String,
    main: PathBuf,
}

fn programs(root: &Path) -> Vec<Program> {
    let mut out = Vec::new();
    for c in discover(root, &["hs", "lhs"]) {
        let dir = c.path.parent().unwrap();
        // A directory with a `Main.hs` is one program of several modules.
        let in_multi = c
            .path
            .ancestors()
            .skip(1)
            .take_while(|d| d.starts_with(root) && *d != root)
            .any(|d| d.join("Main.hs").exists() || d.join("Main.lhs").exists());
        let is_main = matches!(
            c.path.file_name().and_then(|n| n.to_str()),
            Some("Main.hs" | "Main.lhs")
        );
        if is_main {
            let rel = dir.strip_prefix(root).unwrap().display().to_string();
            out.push(Program {
                name: rel,
                main: c.path.clone(),
            });
        } else if !in_multi {
            let rel = c.path.strip_prefix(root).unwrap().with_extension("");
            out.push(Program {
                name: rel.display().to_string(),
                main: c.path.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The expectation files of a program: `<name>.stdout` next to a single
/// file, `stdout` etc. inside a program directory.
fn expect_file(p: &Program, ext: &str) -> PathBuf {
    let stem = p.main.file_stem().unwrap().to_str().unwrap();
    if stem == "Main" {
        p.main.parent().unwrap().join(format!("Main.{ext}"))
    } else {
        p.main.with_extension(ext)
    }
}

/// Runs `exe` with a time limit; `None` as the code on timeout.
fn run_limited(
    exe: &Path,
    cwd: Option<&Path>,
    args: &[String],
    stdin: &[u8],
    limit: Duration,
) -> (String, String, Option<i32>) {
    let mut cmd = Command::new(exe);
    cmd.args(args).env("CIRKA_LEAK_CHECK", "1");
    // Programs read their data files relative to their directory.
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    run_cmd_limited(cmd, stdin, limit)
}

fn run_cmd_limited(
    mut cmd: Command,
    stdin: &[u8],
    limit: Duration,
) -> (String, String, Option<i32>) {
    use std::io::Write;
    let mut child = match cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (String::new(), e.to_string(), Some(-1)),
    };
    let mut si = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    std::thread::spawn(move || {
        let _ = si.write_all(&data);
    });
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let ho = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = so.read_to_end(&mut v);
        v
    });
    let he = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = se.read_to_end(&mut v);
        v
    });
    let start = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st.code().unwrap_or(128 + 9)),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break Some(-1),
        }
    };
    let out = String::from_utf8_lossy(&ho.join().unwrap_or_default()).into_owned();
    let err = String::from_utf8_lossy(&he.join().unwrap_or_default()).into_owned();
    (out, err, code)
}

fn first_line(s: &str) -> String {
    let l = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut l: String = l.chars().take(160).collect();
    l = l.replace('\t', " ");
    l
}

fn measure(cfg: &cirka_driver::Config, p: &Program) -> (Category, String) {
    let root = format!("{}/", repo_root().display());
    let mut sm = SourceMap::new();
    let opts = cirka_driver::pipeline::Options::new(&p.name);
    let roots = vec![p.main.parent().unwrap().to_path_buf(), achc::lib_dir()];
    let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        achc::compile(&mut sm, &p.main, &roots, &opts, "compat")
    }));
    let compiled = match compiled {
        Err(_) => return (Category::Ice, "front end panicked".into()),
        Ok(Err(ds)) => {
            // An unsupported extension explains the other errors.
            let d = ds
                .iter()
                .find(|d| d.code.as_deref() == Some("E0001"))
                .unwrap_or(&ds[0]);
            let code = d.code.clone().unwrap_or_default();
            let text = render(d, &sm, Style::Plain).replace(&root, "");
            return (classify_diag(&code, &d.message), first_line(&text));
        }
        Ok(Ok(c)) => c,
    };
    // Programs with cyclic data (W0401) run with the cycle collector.
    let cyclic = compiled
        .warnings
        .iter()
        .any(|d| d.code.as_deref() == Some("W0401"));
    let mut compiled = compiled;
    compiled.core.collect_cycles = cyclic;
    let mut opts = cirka_driver::pipeline::Options::new(&p.name);
    opts.opt_level = 2;
    let work = cirka_testkit::work_dir("achc-compat", &p.name.replace('/', "_"));
    opts.build_dir = work.clone();
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cirka_driver::pipeline::compile_core(compiled.core, &p.name, &opts, false)
    }));
    let mir = match out {
        Err(_) => return (Category::Ice, "back end panicked".into()),
        Ok(Err(ds)) => return (Category::Ice, first_line(&ds[0].message)),
        Ok(Ok(o)) => o.mir,
    };
    let exe = work.join("prog");
    let nopts = cirka_driver::native::NativeOptions {
        runtime: cirka_driver::native::RuntimeKind::Release,
        ..Default::default()
    };
    if let Err(e) = cirka_driver::native::build_mir(cfg, &mir, &work, &exe, &nopts) {
        return (Category::Ice, first_line(&e));
    }
    let want_out = std::fs::read_to_string(expect_file(p, "stdout")).unwrap_or_default();
    let want_code: i32 = std::fs::read_to_string(expect_file(p, "exitcode"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let stdin = std::fs::read(expect_file(p, "stdin")).unwrap_or_default();
    let args: Vec<String> = std::fs::read_to_string(expect_file(p, "args"))
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();
    let secs = std::env::var("CIRKA_COMPAT_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let (out, err, code) = run_limited(
        &exe,
        p.main.parent(),
        &args,
        &stdin,
        Duration::from_secs(secs),
    );
    let mut cat = classify_run(&want_out, want_code, &out, code, &err);
    if cat == Category::Pass && cyclic {
        cat = Category::Cyclic;
    }
    let detail = match cat {
        Category::Pass => String::new(),
        Category::Cyclic => "passes with --cycles=collect".into(),
        Category::Timeout => format!("no result in {secs} s"),
        Category::Crash => format!("exit {:?}: {}", code, first_line(&err)),
        Category::Output => {
            let d = cirka_testkit::line_diff(&want_out, &out);
            first_line(&d)
        }
        _ => first_line(&err),
    };
    (cat, detail)
}

fn read_baseline(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect()
}

/// Measures one program in a child process (this test binary running
/// [`compat_child`]), so that a compiler crash or stack overflow only
/// affects that program.
fn measure_isolated(p: &Program) -> (Category, String) {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new(exe);
    cmd.args([
        "--ignored",
        "--exact",
        "compat_child",
        "--nocapture",
        "--test-threads=1",
    ])
    .env("CIRKA_COMPAT_CHILD", &p.main)
    .env("CIRKA_COMPAT_CHILD_NAME", &p.name);
    let (out, err, code) = run_cmd_limited(cmd, &[], Duration::from_secs(900));
    for l in out.lines().chain(err.lines()) {
        // The test harness may print `test compat_child ... ` first.
        if let Some(r) = l.split_once("COMPAT\t").map(|x| x.1) {
            let mut it = r.splitn(2, '\t');
            let cat = it.next().unwrap_or("");
            let detail = it.next().unwrap_or("").to_string();
            if let Some(c) = Category::all().into_iter().find(|c| c.name() == cat) {
                return (c, detail);
            }
        }
    }
    let why = match code {
        None => "the compiler did not finish in 900 s".to_string(),
        Some(c) => {
            let msg = err
                .lines()
                .find(|l| l.contains("overflow") || l.contains("panicked"))
                .unwrap_or("")
                .to_string();
            format!("the compiler crashed (exit {c}) {msg}")
        }
    };
    (Category::Ice, first_line(&why))
}

#[test]
#[ignore = "child process of achc_compat"]
fn compat_child() {
    let Ok(main) = std::env::var("CIRKA_COMPAT_CHILD") else {
        return;
    };
    cirka_testkit::init();
    let cfg = cirka_driver::Config::from_env();
    let p = Program {
        name: std::env::var("CIRKA_COMPAT_CHILD_NAME").unwrap_or_default(),
        main: PathBuf::from(main),
    };
    let (c, d) = measure(&cfg, &p);
    println!("COMPAT\t{}\t{}", c.name(), d.replace('\n', " "));
}

#[test]
#[ignore = "slow; run by scripts/compat.sh"]
fn achc_compat() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let root = repo_root().join("tests/achc/compat");
    let filter = std::env::var("CIRKA_COMPAT_FILTER").unwrap_or_default();
    let mut progs = programs(&root);
    // nofib (not committed; see scripts/import-compat.py).
    let nofib = repo_root().join("_build/compat");
    if nofib.join("nofib").is_dir() {
        progs.extend(programs(&nofib));
    }
    let progs: Vec<Program> = progs
        .into_iter()
        .filter(|p| p.name.contains(&filter))
        .collect();
    assert!(!progs.is_empty());
    let results: Mutex<BTreeMap<String, (Category, String)>> = Mutex::new(BTreeMap::new());
    let next = Mutex::new(0usize);
    let jobs = cirka_testkit::jobs().min(progs.len());
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(p) = progs.get(i) else { break };
                    let r = measure_isolated(p);
                    eprintln!("compat: {:<12} {}", r.0.name(), p.name);
                    results.lock().unwrap().insert(p.name.clone(), r);
                }
            });
        }
    });
    let results = results.into_inner().unwrap();
    // Raw results and a summary.
    let mut tsv = String::new();
    for (n, (c, d)) in &results {
        tsv.push_str(&format!("{n}\t{}\t{d}\n", c.name()));
    }
    let adir = repo_root().join("docs/artifacts/compat");
    let _ = std::fs::create_dir_all(&adir);
    let _ = std::fs::write(adir.join("results.tsv"), &tsv);
    let mut summary = String::from("category\tghc\tnofib\n");
    for c in Category::all() {
        let count = |pre: &str| {
            results
                .iter()
                .filter(|(n, (k, _))| n.starts_with(pre) && *k == c)
                .count()
        };
        summary.push_str(&format!(
            "{}\t{}\t{}\n",
            c.name(),
            count("ghc/"),
            count("nofib/")
        ));
    }
    eprintln!("\n{summary}");
    let _ = std::fs::write(adir.join("summary.tsv"), &summary);
    // Regressions against the baseline.
    let bpath = root.join("baseline.tsv");
    let base = read_baseline(&bpath);
    let mut regressions = Vec::new();
    let mut changes = Vec::new();
    for (n, (c, _)) in &results {
        match base.get(n) {
            Some(b) if b == "pass" && *c != Category::Pass => {
                regressions.push(format!("{n}: pass -> {}", c.name()))
            }
            Some(b) if b != c.name() => changes.push(format!("{n}: {b} -> {}", c.name())),
            None => changes.push(format!("{n}: new ({})", c.name())),
            _ => {}
        }
    }
    if !changes.is_empty() {
        eprintln!("changes from the baseline:\n  {}", changes.join("\n  "));
    }
    if cirka_testkit::bless() && filter.is_empty() {
        let mut b = String::from(
            "# Category of each compat program (FEAT-0119). Regenerate with\n\
             # CIRKA_BLESS=1 scripts/compat.sh after reviewing the changes.\n",
        );
        let mut all: BTreeMap<String, String> = base.clone();
        for (n, (c, _)) in &results {
            all.insert(n.clone(), c.name().to_string());
        }
        for (n, c) in &all {
            b.push_str(&format!("{n}\t{c}\n"));
        }
        std::fs::write(&bpath, b).unwrap();
        return;
    }
    assert!(
        regressions.is_empty(),
        "regressions:\n  {}",
        regressions.join("\n  ")
    );
}
