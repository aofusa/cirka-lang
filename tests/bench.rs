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
    let mut opts = cirka_driver::pipeline::Options::new("bench");
    // Without Z3, the contracts are checked at run time (W0507), which
    // prints the same.
    let no_z3 = cirka_smt::Solver::from_env().is_none();
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
        if no_z3 {
            opts.frontend_flags.push("--contracts=check".into());
        }
        match cirka::compile(&mut sm, path, &roots, &opts, "Main") {
            Ok(c)
                if c.warnings
                    .iter()
                    .all(|w| no_z3 && w.code.as_deref() == Some("W0507")) =>
            {
                Ok(c.core)
            }
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
    // In-place updates of the bench programs never copy (BUG-0032: a kept
    // reference made every write of bench/array-sort copy the array).
    if r.stats.array_copies != 0 {
        return Err(format!(
            "{} copies of shared arrays by in-place updates",
            r.stats.array_copies
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

/// The answer of the HTTP benchmark's servers to `GET` (FEAT-0243).
const HTTP_HELLO: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nHello, World!";

/// Talks to the HTTP server on `port`: pipelined requests, a request
/// split across writes, and another method (400, then closed).
fn http_exchange(port: u16) -> Result<(), String> {
    use std::io::{Read, Write};
    let connect = || -> Result<std::net::TcpStream, String> {
        let s = std::net::TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        Ok(s)
    };
    let read = |s: &mut std::net::TcpStream, n: usize| -> Result<Vec<u8>, String> {
        let mut got = vec![0u8; n];
        s.read_exact(&mut got)
            .map_err(|e| format!("reading {n} bytes: {e}"))?;
        Ok(got)
    };
    let mut s = connect()?;
    s.write_all(b"GET /a HTTP/1.1\r\nHost: localhost\r\n\r\nGET /b HTTP/1.1\r\n\r\n")
        .map_err(|e| e.to_string())?;
    if read(&mut s, 2 * HTTP_HELLO.len())? != [HTTP_HELLO, HTTP_HELLO].concat() {
        return Err("pipelined requests: wrong answers".into());
    }
    s.write_all(b"GET /split HT").map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(20));
    s.write_all(b"TP/1.1\r\n\r\n").map_err(|e| e.to_string())?;
    if read(&mut s, HTTP_HELLO.len())? != HTTP_HELLO {
        return Err("split request: wrong answer".into());
    }
    s.write_all(b"POST / HTTP/1.1\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut rest = Vec::new();
    s.read_to_end(&mut rest).map_err(|e| e.to_string())?;
    let bad: &[u8] = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    if rest != bad {
        return Err(format!(
            "another method: answered {:?}",
            String::from_utf8_lossy(&rest)
        ));
    }
    Ok(())
}

/// Runs the server `exe` on a free port (printed first) and gives the port
/// to `f`; the server is killed afterwards.
fn with_server(exe: &Path, f: impl FnOnce(u16) -> Result<(), String>) -> Result<(), String> {
    use std::io::BufRead;
    let mut child = std::process::Command::new(exe)
        .arg("0")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let mut line = String::new();
    let out = child.stdout.take().unwrap();
    let r = std::io::BufReader::new(out)
        .read_line(&mut line)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            line.trim()
                .parse::<u16>()
                .map_err(|_| format!("printed {line:?} instead of the port"))
        })
        .and_then(f);
    let _ = child.kill();
    let _ = child.wait();
    r
}

/// The servers of the HTTP benchmark (bench/http-server/, FEAT-0243)
/// answer as `load.rs` expects, and the load runs against the CirkaLang
/// one. The Rust server and the load need `rustc` (in the Docker image).
#[test]
fn http_server() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let dir = repo_root().join("bench/http-server");
    let work = cirka_testkit::work_dir("bench", "http-server");
    let core = compile(&dir.join("server.cirka")).unwrap();
    let mut opts = cirka_driver::pipeline::Options::new("http-server");
    opts.opt_level = 2;
    opts.build_dir = work.clone();
    let out = cirka_driver::pipeline::compile_core(core, "http-server", &opts, false)
        .map_err(|ds| ds[0].message.clone())
        .unwrap();
    let cirka = work.join("server-cirka");
    let native = cirka_driver::native::NativeOptions {
        runtime: cirka_driver::native::RuntimeKind::Debug,
        ..Default::default()
    };
    cirka_driver::native::build_mir(&cfg, &out.mir, &work, &cirka, &native).unwrap();
    let rustc = |src: &str, exe: &str| -> Option<PathBuf> {
        let exe = work.join(exe);
        let st = std::process::Command::new("rustc")
            .args(["-O", "-o"])
            .arg(&exe)
            .arg(dir.join(src))
            .status()
            .ok()?;
        assert!(st.success(), "rustc failed on {src}");
        Some(exe)
    };
    let load = rustc("load.rs", "load");
    with_server(&cirka, |port| {
        http_exchange(port)?;
        if let Some(load) = &load {
            let out = std::process::Command::new(load)
                .args([port.to_string(), "4".into(), "200".into()])
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() || !text.starts_with("requests=800 ") {
                return Err(format!(
                    "load: {text}{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
        Ok(())
    })
    .unwrap();
    if let Some(rust) = rustc("rust/server.rs", "server-rust") {
        with_server(&rust, http_exchange).unwrap();
    }
}
