//! cirka-testkit: the shared integration test harness (10-testing.md §2).
//!
//! - discovers test files and parses their header comments,
//! - compares output with expectation files (`ACHC_BLESS=1` / `CIRKA_BLESS=1`
//!   rewrites them),
//! - runs cases in parallel (`CIRKA_TEST_JOBS`),
//! - keeps artifacts of failing cases under `_build/test-failures/<name>/`,
//! - caches the reference outputs of GHC for differential tests.
//!
//! This crate deliberately has no dependencies on the compiler crates so that
//! every crate can use it as a dev-dependency.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Root of the repository (the workspace directory).
pub fn repo_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Options given in the leading comment lines of a test file, e.g.
///
/// ```text
/// -- flags: -O2 --ownership=strict
/// -- expect-leak: 1
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub entries: BTreeMap<String, String>,
}

impl Header {
    /// Parses `key: value` pairs from leading comment lines. Comment markers
    /// `--`, `//`, `;` and `#` are accepted. Parsing stops at the first line
    /// that is not a comment.
    pub fn parse(text: &str) -> Header {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let t = line.trim();
            // Haskell `{-# LANGUAGE ... #-}` pragmas may precede the header.
            if t.is_empty() || (t.starts_with("{-#") && t.ends_with("#-}")) {
                continue;
            }
            let body = ["--", "//", ";", "#"]
                .iter()
                .find_map(|m| t.strip_prefix(m));
            let Some(body) = body else { break };
            let body = body.trim();
            if let Some((k, v)) = body.split_once(':') {
                let k = k.trim();
                if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    entries.insert(k.to_string(), v.trim().to_string());
                }
            }
        }
        Header { entries }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Whitespace separated `flags:` value.
    pub fn flags(&self) -> Vec<String> {
        self.get("flags")
            .map(|f| f.split_whitespace().map(String::from).collect())
            .unwrap_or_default()
    }

    /// `KEY=VALUE` pairs from `runtime-env:`.
    pub fn runtime_env(&self) -> Vec<(String, String)> {
        self.get("runtime-env")
            .map(|f| {
                f.split_whitespace()
                    .filter_map(|kv| kv.split_once('='))
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// One discovered test case.
#[derive(Clone, Debug)]
pub struct Case {
    pub path: PathBuf,
    /// Path relative to the suite directory, without extension, using `/`.
    pub name: String,
    pub header: Header,
}

impl Case {
    /// Path of a sibling expectation file such as `foo.stdout`.
    pub fn sibling(&self, ext: &str) -> PathBuf {
        self.path.with_extension(ext)
    }

    pub fn source(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

/// Recursively collects files under `dir` with one of `exts`, sorted by path.
pub fn discover(dir: &Path, exts: &[&str]) -> Vec<Case> {
    let mut files = Vec::new();
    walk(dir, exts, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let rel = path.strip_prefix(dir).unwrap_or(&path).with_extension("");
            let name = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let header = Header::parse(&std::fs::read_to_string(&path).unwrap_or_default());
            Case { path, name, header }
        })
        .collect()
}

fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, exts, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.contains(&e))
        {
            out.push(p);
        }
    }
}

/// True when expectation files should be rewritten.
pub fn bless() -> bool {
    ["CIRKA_BLESS", "ACHC_BLESS"]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Compares `actual` with the contents of `expected_path`.
///
/// In bless mode the file is (re)written and the comparison succeeds. An
/// empty `actual` with a missing file counts as a match (so most tests need
/// no `.stderr` file).
pub fn check_expect(expected_path: &Path, actual: &str) -> Result<(), String> {
    check_expect_with(expected_path, actual, bless())
}

pub fn check_expect_with(expected_path: &Path, actual: &str, bless: bool) -> Result<(), String> {
    let expected = std::fs::read_to_string(expected_path).ok();
    if bless {
        match (&expected, actual.is_empty()) {
            (None, true) => {}
            (Some(_), true) => {
                let _ = std::fs::remove_file(expected_path);
            }
            _ => {
                if expected.as_deref() != Some(actual) {
                    std::fs::write(expected_path, actual)
                        .map_err(|e| format!("cannot write {}: {e}", expected_path.display()))?;
                }
            }
        }
        return Ok(());
    }
    let expected = expected.unwrap_or_default();
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "output differs from {} (set CIRKA_BLESS=1 to update)\n{}",
            expected_path.display(),
            line_diff(&expected, actual)
        ))
    }
}

/// A small line diff (`-` expected, `+` actual) for failure messages.
pub fn line_diff(expected: &str, actual: &str) -> String {
    let a: Vec<&str> = expected.lines().collect();
    let b: Vec<&str> = actual.lines().collect();
    // LCS table (inputs are small).
    let n = a.len();
    let m = b.len();
    if n * m > 4_000_000 {
        return format!("--- expected ({n} lines)\n+++ actual ({m} lines)\n");
    }
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i] == b[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    let mut shown = 0;
    while (i < n || j < m) && shown < 200 {
        if i < n && j < m && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < m && (i == n || t[i][j + 1] >= t[i + 1][j]) {
            out.push_str(&format!("+{}\n", b[j]));
            j += 1;
            shown += 1;
        } else {
            out.push_str(&format!("-{}\n", a[i]));
            i += 1;
            shown += 1;
        }
    }
    if expected.ends_with('\n') != actual.ends_with('\n') {
        out.push_str("(trailing newline differs)\n");
    }
    out
}

/// Result of a single case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
}

/// Number of worker threads (`CIRKA_TEST_JOBS`, 0 = number of CPUs).
pub fn jobs() -> usize {
    let n = std::env::var("CIRKA_TEST_JOBS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    if n > 0 {
        n
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }
}

/// Summary of a suite run.
#[derive(Debug, Default)]
pub struct Report {
    pub passed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{} passed, {} failed, {} skipped\n",
            self.passed.len(),
            self.failed.len(),
            self.skipped.len()
        );
        for (n, m) in &self.failed {
            s.push_str(&format!("\n=== FAIL {n}\n{m}\n"));
        }
        s
    }
}

/// Stack size of the threads that run compilers (tests and CLIs).
pub const COMPILER_STACK: usize = 256 << 20;

/// The limits of a run of cases (BUG-0041). The compilers and the MIR
/// interpreter run in the test process: a case where one of them does not
/// terminate would hold its thread, and the memory it takes, forever.
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// The longest a case may run (`CIRKA_TEST_CASE_TIMEOUT`, seconds).
    case_timeout: Duration,
    /// The most memory the test process may hold (`CIRKA_TEST_MAX_RSS_MB`).
    max_rss_mb: u64,
}

impl Limits {
    /// From the environment: `CIRKA_TEST_CASE_TIMEOUT` (1800 seconds when
    /// unset) and `CIRKA_TEST_MAX_RSS_MB` (three quarters of the memory
    /// the process may use, [`memory_mb`], when unset).
    fn from_env() -> Limits {
        let num = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .filter(|&n| n > 0)
        };
        Limits {
            case_timeout: Duration::from_secs(num("CIRKA_TEST_CASE_TIMEOUT").unwrap_or(1800)),
            max_rss_mb: num("CIRKA_TEST_MAX_RSS_MB")
                .unwrap_or_else(|| memory_mb().map_or(6144, |mb| mb / 4 * 3)),
        }
    }
}

/// The memory this process may use in megabytes: the machine's
/// (`MemTotal`), or its cgroup's limit (`memory.max`, a container's) when
/// smaller.
fn memory_mb() -> Option<u64> {
    let total = proc_kb("/proc/meminfo", "MemTotal:").map(|kb| kb / 1024);
    let cgroup = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|bytes| bytes >> 20);
    total.into_iter().chain(cgroup).min()
}

/// The number of kilobytes on the line `key` of a file such as
/// `/proc/self/status`.
fn proc_kb(path: &str, key: &str) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix(key))?
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()
}

/// The resident memory of this process in megabytes (`VmRSS`).
fn rss_mb() -> Option<u64> {
    proc_kb("/proc/self/status", "VmRSS:").map(|kb| kb / 1024)
}

/// The case each thread of [`run_cases`] runs, with its start.
type Running = Mutex<Vec<Option<(String, Instant)>>>;

/// Watches the cases running until every thread of [`run_cases`] has
/// dropped its sender of `alive`: a case over [`Limits::case_timeout`], or
/// the process over [`Limits::max_rss_mb`], ends the process with the
/// names of the cases running (BUG-0041).
fn watch(limits: Limits, running: &Running, alive: std::sync::mpsc::Receiver<()>) {
    use std::sync::mpsc::RecvTimeoutError;
    while let Err(RecvTimeoutError::Timeout) = alive.recv_timeout(Duration::from_millis(250)) {
        let r = running.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let late = r
            .iter()
            .flatten()
            .find(|(_, t)| t.elapsed() > limits.case_timeout);
        let why = if let Some((n, _)) = late {
            format!(
                "the case `{n}` has run for more than {} s (CIRKA_TEST_CASE_TIMEOUT)",
                limits.case_timeout.as_secs()
            )
        } else if let Some(mb) = rss_mb().filter(|&mb| mb > limits.max_rss_mb) {
            format!(
                "the test process holds {mb} MB, more than {} MB (CIRKA_TEST_MAX_RSS_MB)",
                limits.max_rss_mb
            )
        } else {
            continue;
        };
        let names: Vec<String> = r
            .iter()
            .flatten()
            .map(|(n, t)| format!("`{n}` ({} s)", t.elapsed().as_secs()))
            .collect();
        // Straight to the standard error: the test harness keeps what
        // `eprintln!` prints until the test ends, which this one does not.
        use std::io::Write as _;
        let _ = writeln!(
            std::io::stderr(),
            "cirka-testkit: {why}; running: {}. Stopping the tests (BUG-0041).",
            names.join(", ")
        );
        std::process::exit(1);
    }
}

/// Runs `f` for every case on `jobs()` threads. An optional filter can be
/// given with `CIRKA_TEST_FILTER=<substring>`. A case over the [`Limits`]
/// stops the process.
pub fn run_cases<F>(cases: &[Case], f: F) -> Report
where
    F: Fn(&Case) -> Outcome + Sync,
{
    let filter = std::env::var("CIRKA_TEST_FILTER").ok();
    let cases: Vec<&Case> = cases
        .iter()
        .filter(|c| filter.as_deref().is_none_or(|p| c.name.contains(p)))
        .collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Outcome)>> = Mutex::new(Vec::new());
    let n = jobs().min(cases.len().max(1));
    let running: Running = Mutex::new(vec![None; n]);
    let limits = Limits::from_env();
    // Each thread holds a sender until it ends (also by unwinding); the
    // watch ends when none is left.
    let (alive, threads_alive) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|s| {
        s.spawn(|| watch(limits, &running, threads_alive));
        for w in 0..n {
            let alive = alive.clone();
            let (next, cases, results, f, running) = (&next, &cases, &results, &f, &running);
            // Compilers recurse over deeply nested terms; use large stacks
            // (reserved lazily) like the CLIs do.
            std::thread::Builder::new()
                .stack_size(COMPILER_STACK)
                .spawn_scoped(s, move || {
                    let _alive = alive;
                    let set = |v: Option<(String, Instant)>| {
                        running.lock().unwrap_or_else(|e| e.into_inner())[w] = v;
                    };
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= cases.len() {
                            break;
                        }
                        let c = cases[i];
                        set(Some((c.name.clone(), Instant::now())));
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(c)))
                            .unwrap_or_else(|e| {
                                let msg = e
                                    .downcast_ref::<String>()
                                    .cloned()
                                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                                    .unwrap_or_default();
                                Outcome::Fail(format!("panicked: {msg}"))
                            });
                        set(None);
                        results.lock().unwrap().push((i, r));
                    }
                })
                .expect("cannot spawn a test thread");
        }
        drop(alive);
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);
    let mut rep = Report::default();
    for (i, r) in results {
        let name = cases[i].name.clone();
        match r {
            Outcome::Pass => rep.passed.push(name),
            Outcome::Fail(m) => rep.failed.push((name, m)),
            Outcome::Skip(m) => rep.skipped.push((name, m)),
        }
    }
    rep
}

/// Runs a suite and panics with a summary if any case failed.
pub fn run_suite<F>(suite: &str, cases: &[Case], f: F)
where
    F: Fn(&Case) -> Outcome + Sync,
{
    let rep = run_cases(cases, f);
    eprintln!("[{suite}] {}", rep.summary().lines().next().unwrap_or(""));
    for (n, m) in &rep.skipped {
        eprintln!("[{suite}] skipped {n}: {m}");
    }
    if !rep.failed.is_empty() {
        panic!("[{suite}] {}", rep.summary());
    }
}

/// Directory where artifacts of a failed case are kept.
pub fn failure_dir(suite: &str, case: &str) -> PathBuf {
    repo_root()
        .join("_build")
        .join("test-failures")
        .join(suite)
        .join(case.replace('/', "__"))
}

/// Saves named artifacts (e.g. generated `.ll`, dumps) of a failed case.
pub fn save_failure(suite: &str, case: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = failure_dir(suite, case);
    let _ = std::fs::create_dir_all(&dir);
    for (name, content) in files {
        let _ = std::fs::write(dir.join(name), content);
    }
    dir
}

/// A per-case scratch directory under `_build/test-work/`.
pub fn work_dir(suite: &str, case: &str) -> PathBuf {
    let dir = repo_root()
        .join("_build")
        .join("test-work")
        .join(suite)
        .join(case.replace('/', "__"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// FNV-1a 64-bit hash (stable across runs; used for caches).
pub fn fnv64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Output of running a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

/// The exit code of a program run by [`valgrind_command`] when valgrind
/// found errors (no test expects it: they exit with 0, 1 or 3).
pub const VALGRIND_ERROR_CODE: i32 = 97;

/// Whether the native programs of the tests run under valgrind's memcheck
/// (`CIRKA_VALGRIND=1`, set by `scripts/memcheck.sh`: BUG-0038).
pub fn under_valgrind() -> bool {
    std::env::var("CIRKA_VALGRIND").is_ok_and(|v| v == "1")
}

/// A command running `exe` under valgrind's memcheck (`CIRKA_VALGRIND_BIN`):
/// errors (invalid reads and writes, uses of uninitialized values, bad
/// frees) make it exit with [`VALGRIND_ERROR_CODE`]. Leaks are left to
/// the runtime's own check (`CIRKA_LEAK_CHECK=1`).
pub fn valgrind_command(exe: &Path) -> Command {
    let bin = std::env::var("CIRKA_VALGRIND_BIN")
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "valgrind".into());
    let mut cmd = Command::new(bin);
    cmd.args([
        "-q",
        &format!("--error-exitcode={VALGRIND_ERROR_CODE}"),
        "--leak-check=no",
    ])
    .arg(exe);
    cmd
}

/// The report of a run by [`valgrind_command`] in which valgrind found
/// errors.
pub fn valgrind_errors(out: &RunOutput) -> Option<&str> {
    (out.code == VALGRIND_ERROR_CODE && out.stderr.contains("==")).then_some(out.stderr.as_str())
}

/// Runs `cmd` with optional stdin, capturing output.
pub fn run_with_stdin(mut cmd: Command, stdin: Option<&[u8]>) -> std::io::Result<RunOutput> {
    use std::io::Write;
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(data) = stdin {
        let mut si = child.stdin.take().unwrap();
        let data = data.to_vec();
        std::thread::spawn(move || {
            let _ = si.write_all(&data);
        });
    }
    let out = child.wait_with_output()?;
    Ok(RunOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(128 + 9),
    })
}

/// Runs a Haskell program with GHC (`CIRKA_GHC`) and caches the result under
/// `_build/ghc-cache/`. Returns `None` when `CIRKA_GHC` is unset or empty.
pub fn ghc_reference(
    src: &Path,
    stdin: Option<&[u8]>,
    args: &[String],
) -> Option<Result<RunOutput, String>> {
    ghc_reference_with(src, stdin, args, &[])
}

/// [`ghc_reference`] with more arguments for GHC (`ghc_args`: C sources to
/// compile with the program, `-l` flags). The contents of the files among
/// them are part of the cache key.
pub fn ghc_reference_with(
    src: &Path,
    stdin: Option<&[u8]>,
    args: &[String],
    ghc_args: &[String],
) -> Option<Result<RunOutput, String>> {
    let ghc = std::env::var("CIRKA_GHC").ok().filter(|s| !s.is_empty())?;
    let text = std::fs::read(src).ok()?;
    let mut key = text.clone();
    // A `Main.hs` is a program of several modules: the other modules of
    // its directory take part in the cache key.
    let dir = src.parent().unwrap_or(Path::new("."));
    if src.file_name().is_some_and(|n| n == "Main.hs") {
        let mut mods = Vec::new();
        walk(dir, &["hs"], &mut mods);
        mods.sort();
        for m in mods.iter().filter(|m| m.as_path() != src) {
            key.extend_from_slice(
                m.strip_prefix(dir)
                    .unwrap_or(m)
                    .to_string_lossy()
                    .as_bytes(),
            );
            key.extend_from_slice(&std::fs::read(m).unwrap_or_default());
        }
    }
    key.extend_from_slice(stdin.unwrap_or_default());
    for a in args {
        key.extend_from_slice(a.as_bytes());
        key.push(0);
    }
    for a in ghc_args {
        key.extend_from_slice(a.as_bytes());
        key.push(1);
        if let Ok(text) = std::fs::read(a) {
            key.extend_from_slice(&text);
        }
    }
    let h = fnv64(&key);
    let cache = repo_root().join("_build").join("ghc-cache");
    let _ = std::fs::create_dir_all(&cache);
    let cfile = cache.join(format!("{h:016x}.out"));
    if let Ok(s) = std::fs::read_to_string(&cfile)
        && let Some(r) = decode_cached(&s)
    {
        return Some(Ok(r));
    }
    let work = cache.join(format!("{h:016x}.d"));
    let _ = std::fs::create_dir_all(&work);
    let exe = work.join("prog");
    let out = Command::new(&ghc)
        .arg("-O0")
        .arg("-v0")
        .arg("-outputdir")
        .arg(&work)
        .arg("-o")
        .arg(&exe)
        .arg(format!("-i{}", dir.display()))
        .args(ghc_args)
        .arg(src)
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Some(Err(format!("cannot run {ghc}: {e}"))),
    };
    if !out.status.success() {
        return Some(Err(format!(
            "ghc failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let mut cmd = Command::new(&exe);
    cmd.args(args);
    let r = match run_with_stdin(cmd, stdin) {
        Ok(r) => r,
        Err(e) => return Some(Err(e.to_string())),
    };
    let _ = std::fs::write(&cfile, encode_cached(&r));
    let _ = std::fs::remove_dir_all(&work);
    Some(Ok(r))
}

fn encode_cached(r: &RunOutput) -> String {
    format!("{}\n{}\n{}{}", r.code, r.stdout.len(), r.stdout, r.stderr)
}

fn decode_cached(s: &str) -> Option<RunOutput> {
    let (code, rest) = s.split_once('\n')?;
    let (len, rest) = rest.split_once('\n')?;
    let len: usize = len.parse().ok()?;
    if rest.len() < len || !rest.is_char_boundary(len) {
        return None;
    }
    Some(RunOutput {
        code: code.parse().ok()?,
        stdout: rest[..len].to_string(),
        stderr: rest[len..].to_string(),
    })
}

/// Loads `.env` (or `.env.sample`) from the repository root into the
/// process environment for keys that are not already set. Used by tests so
/// that `cargo test` outside of `scripts/` sees the same settings.
pub fn load_dotenv() {
    let root = repo_root();
    let text = std::fs::read_to_string(root.join(".env"))
        .or_else(|_| std::fs::read_to_string(root.join(".env.sample")))
        .unwrap_or_default();
    for (k, v) in parse_dotenv(&text) {
        if std::env::var_os(&k).is_none() {
            set_env(&k, &v);
        }
    }
}

#[allow(unsafe_code)]
fn set_env(k: &str, v: &str) {
    // SAFETY: called once at test start-up, before any worker threads that
    // read the environment are spawned (guarded by `std::sync::Once`).
    unsafe { std::env::set_var(k, v) }
}

/// Parses `KEY=VALUE` lines (comments and blank lines ignored, optional quotes).
pub fn parse_dotenv(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let t = t.strip_prefix("export ").unwrap_or(t);
        if let Some((k, v)) = t.split_once('=') {
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(v);
            out.push((k.trim().to_string(), v.to_string()));
        }
    }
    out
}

/// Calls [`load_dotenv`] once per process.
pub fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(load_dotenv);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parsing() {
        let h = Header::parse(
            "-- flags: -O2 --ownership=strict\n-- runtime-env: CIRKA_STACK_SIZE=64M\n-- expect-leak: 1\nmodule Main where\n-- flags: ignored\n",
        );
        assert_eq!(h.flags(), vec!["-O2", "--ownership=strict"]);
        assert_eq!(h.get("expect-leak"), Some("1"));
        assert_eq!(
            h.runtime_env(),
            vec![("CIRKA_STACK_SIZE".into(), "64M".into())]
        );
        assert_eq!(h.entries.len(), 3);
        let h =
            Header::parse("{-# LANGUAGE LinearTypes #-}\n-- max-allocs: 3\nmodule Main where\n");
        assert_eq!(h.get("max-allocs"), Some("3"));
        let h = Header::parse("// skip-backend: llvm\nfn main() {}");
        assert_eq!(h.get("skip-backend"), Some("llvm"));
    }

    #[test]
    fn expect_pass_fail_bless() {
        let dir = std::env::temp_dir().join(format!("cirka-testkit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.stdout");
        // missing file + empty output = ok
        assert!(check_expect_with(&p, "", false).is_ok());
        // missing file + output = fail
        assert!(check_expect_with(&p, "hi\n", false).is_err());
        // bless writes
        assert!(check_expect_with(&p, "hi\n", true).is_ok());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hi\n");
        assert!(check_expect_with(&p, "hi\n", false).is_ok());
        let e = check_expect_with(&p, "ho\n", false).unwrap_err();
        assert!(e.contains("-hi") && e.contains("+ho"), "{e}");
        // bless with empty output removes the file
        assert!(check_expect_with(&p, "", true).is_ok());
        assert!(!p.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parallel_runner_reports() {
        let cases: Vec<Case> = (0..20)
            .map(|i| Case {
                path: PathBuf::from(format!("{i}.t")),
                name: format!("c{i:02}"),
                header: Header::default(),
            })
            .collect();
        let rep = run_cases(&cases, |c| match c.name.as_str() {
            "c03" => Outcome::Fail("bad".into()),
            "c05" => Outcome::Skip("no ghc".into()),
            "c07" => panic!("boom"),
            _ => Outcome::Pass,
        });
        assert_eq!(rep.passed.len(), 17);
        assert_eq!(rep.failed.len(), 2);
        assert_eq!(rep.failed[0].0, "c03");
        assert!(rep.failed[1].1.contains("boom"));
        assert_eq!(rep.skipped.len(), 1);
    }

    #[test]
    fn diff_and_cache_encoding() {
        let d = line_diff("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(d, "+x\n-b\n");
        let r = RunOutput {
            stdout: "1\n2\n".into(),
            stderr: "err\n".into(),
            code: 3,
        };
        assert_eq!(decode_cached(&encode_cached(&r)), Some(r));
    }

    #[test]
    fn dotenv() {
        let v = parse_dotenv("# c\nA=1\nexport B=\"x y\"\n\nC=\n");
        assert_eq!(
            v,
            vec![
                ("A".into(), "1".into()),
                ("B".into(), "x y".into()),
                ("C".into(), "".into())
            ]
        );
    }

    #[test]
    fn discover_files() {
        let dir = std::env::temp_dir().join(format!("cirka-testkit-d-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("b.mir"), "// flags: -O1\n").unwrap();
        std::fs::write(dir.join("sub/a.mir"), "").unwrap();
        std::fs::write(dir.join("c.txt"), "").unwrap();
        let cs = discover(&dir, &["mir"]);
        let names: Vec<_> = cs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["b", "sub/a"]);
        assert_eq!(cs[0].header.flags(), vec!["-O1"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
