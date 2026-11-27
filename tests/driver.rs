//! Shared driver tests (FEAT-0032): both CLIs (`achc` and `cirka`) compile
//! Core input through the same pipeline and give the same results.

use cirka_testkit::{Outcome, discover, repo_root, run_suite};
use std::path::PathBuf;
use std::process::Command;

fn run(bin: &PathBuf, args: &[&str], cwd: &std::path::Path) -> cirka_testkit::RunOutput {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd).env("CIRKA_LEAK_CHECK", "1");
    cirka_testkit::run_with_stdin(cmd, None).unwrap()
}

/// `achc run` and `cirka run` on every tests/core/run program print the
/// expected output with the same exit code, at -O0 and -O2. Programs with
/// a C helper (FEAT-0047) link it with the linker flags of the CLIs.
#[test]
fn both_clis_agree_on_core_input() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let cfg = cirka_driver::Config::from_env();
    let (achc, cirka) = cirka_integration::cli_bins();
    let cases = discover(&repo_root().join("tests/core/run"), &["core"]);
    assert!(!cases.is_empty());
    run_suite("driver-core", &cases, |c| {
        let link = match cirka_integration::c_helper(&cfg, c, "driver-core") {
            Ok(l) => l,
            Err(e) => return Outcome::Fail(e),
        };
        let expected = std::fs::read_to_string(c.sibling("stdout")).unwrap_or_default();
        // Without `.exitcode`, `main`'s result is the exit code: all runs
        // must agree with each other.
        let mut code: Option<i32> = std::fs::read_to_string(c.sibling("exitcode"))
            .ok()
            .and_then(|s| s.trim().parse().ok());
        let src = c.path.to_str().unwrap();
        let mut outs = Vec::new();
        for (tool, bin) in [("achc", achc), ("cirka", cirka)] {
            for (o, rt) in [("-O0", "--runtime=debug"), ("-O2", "--runtime=release")] {
                let work = cirka_testkit::work_dir("driver-core", &format!("{}-{tool}{o}", c.name));
                let bd = format!("--build-dir={}", work.display());
                let mut args = vec!["run", o, rt, &bd];
                args.extend(link.iter().map(String::as_str));
                args.push(src);
                let out = run(bin, &args, &work);
                let want = *code.get_or_insert(out.code);
                if out.stdout != expected || out.code != want {
                    return Outcome::Fail(format!(
                        "{tool} {o}: exit {} (expected {want})\n{}\n{}",
                        out.code,
                        cirka_testkit::line_diff(&expected, &out.stdout),
                        out.stderr
                    ));
                }
                if out.stderr.contains("leaked") {
                    return Outcome::Fail(format!("{tool} {o}: {}", out.stderr));
                }
                outs.push(out);
            }
        }
        if outs.windows(2).any(|w| w[0].stdout != w[1].stdout) {
            return Outcome::Fail("CLIs disagree".into());
        }
        Outcome::Pass
    });
}

/// The final MIR (`--dump=mir-opt`) of aCHC and CirkaLang programs reads
/// back (BUG-0045): it has operator names (`Cirka.Base.++`) and names with
/// dots (`Prelude.Nil`). The parsed module verifies, and printing it gives
/// the same text.
#[test]
fn optimized_mir_of_both_frontends_reads_back() {
    cirka_testkit::init();
    let (achc, cirka) = cirka_integration::cli_bins();
    let mut quoted = 0;
    for (bin, src) in [
        (achc, "bench/spectral-norm/plain.hs"),
        (achc, "bench/fannkuch-redux/plain.hs"),
        (cirka, "bench/spectral-norm/main.cirka"),
        (cirka, "bench/cli-grep/main.cirka"),
    ] {
        let path = repo_root().join(src);
        let work = cirka_testkit::work_dir("driver-mir-text", &src.replace('/', "-"));
        let bd = format!("--build-dir={}", work.display());
        let ll = work.join("out.ll");
        let out = run(
            bin,
            &[
                "build",
                "-O2",
                "--emit=llvm",
                "--dump=mir-opt",
                &bd,
                "-o",
                ll.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
            &work,
        );
        assert_eq!(out.code, 0, "{src}: {}", out.stderr);
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let dump = work.join(format!("{stem}.opt.mir"));
        let text = std::fs::read_to_string(&dump)
            .unwrap_or_else(|e| panic!("{src}: {}: {e}", dump.display()));
        quoted += text.matches('`').count();
        let m = cirka_mir::parse(cirka_diagnostics::FileId(0), &text)
            .unwrap_or_else(|e| panic!("{src}: {}", e.message));
        let errs = cirka_mir::verify::verify_module(&m);
        assert!(
            errs.is_empty(),
            "{src}: {}",
            cirka_mir::verify::render_errors(&errs)
        );
        let again = cirka_mir::print_module(&m);
        assert!(
            text == again,
            "{src}:\n{}",
            cirka_testkit::line_diff(&text, &again)
        );
    }
    assert!(
        quoted > 0,
        "no quoted name: the programs no longer cover BUG-0045"
    );
}

/// `check` reports Lint errors of a `.core` file as diagnostics (exit 1)
/// identically from both CLIs, and `--dump` writes every stage.
#[test]
fn check_and_dump() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (achc, cirka) = cirka_integration::cli_bins();
    let fail = discover(&repo_root().join("tests/core/fail"), &["core"]);
    let bad = fail[0].path.to_str().unwrap().to_string();
    let work = cirka_testkit::work_dir("driver-misc", "check");
    let a = run(achc, &["check", &bad], &work);
    let b = run(cirka, &["check", &bad], &work);
    assert_eq!(a.code, 1, "{}", a.stderr);
    assert!(a.stderr.contains("error"), "{}", a.stderr);
    assert_eq!(a, b);

    let ok = repo_root().join("tests/core/run/haskell_style.core");
    let bd = work.join("dump");
    let out = run(
        cirka,
        &[
            "build",
            "--dump=all",
            &format!("--build-dir={}", bd.display()),
            "-o",
            work.join("hs").to_str().unwrap(),
            ok.to_str().unwrap(),
        ],
        &work,
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    for ext in ["core", "opt.core", "mir", "opt.mir", "ll"] {
        let p = bd.join(format!("haskell_style.{ext}"));
        assert!(p.exists(), "missing dump {}", p.display());
    }
    let r = run(&work.join("hs"), &[], &work);
    assert_eq!(
        r.stdout,
        std::fs::read_to_string(ok.with_extension("stdout")).unwrap()
    );

    let u = run(achc, &["frobnicate", "x.core"], &work);
    assert_eq!(u.code, 2);
    assert!(u.stderr.contains("unknown command"));
}

/// `cirka test` on tests/cirkalang/test/*.cirka (FEAT-0216): each test runs
/// in its own process; the report is `.stdout` and the exit code
/// `.exitcode` (default 0). `--filter=` selects tests by name.
#[test]
fn cirka_source_tests() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (achc, cirka) = cirka_integration::cli_bins();
    let cases = discover(&repo_root().join("tests/cirkalang/test"), &["cirka"]);
    assert!(!cases.is_empty());
    run_suite("cirka-test", &cases, |c| {
        let work = cirka_testkit::work_dir("cirka-test", &c.name);
        let bd = format!("--build-dir={}", work.display());
        let dir = c.path.parent().unwrap();
        let file = c.path.file_name().unwrap().to_str().unwrap();
        let out = run(cirka, &["test", &bd, file], dir);
        let want: i32 = std::fs::read_to_string(c.sibling("exitcode"))
            .map(|s| s.trim().parse().unwrap())
            .unwrap_or(0);
        if out.code != want {
            return Outcome::Fail(format!(
                "exit {} (expected {want})\n{}{}",
                out.code, out.stdout, out.stderr
            ));
        }
        if let Err(e) = cirka_testkit::check_expect(&c.sibling("stdout"), &out.stdout) {
            return Outcome::Fail(e);
        }
        // Filtering, and the same file still builds and runs as a program.
        let f = run(cirka, &["test", &bd, "--filter=twice", file], dir);
        if !f.stdout.contains("filtered out") || f.code != 0 && want == 0 {
            return Outcome::Fail(format!("--filter: {}{}", f.stdout, f.stderr));
        }
        Outcome::Pass
    });
    // Haskell sources have no tests in source files.
    let hs = repo_root().join("tests/achc/run/h1/fact.hs");
    let work = cirka_testkit::work_dir("cirka-test", "achc");
    let bd = format!("--build-dir={}", work.display());
    let out = run(achc, &["test", &bd, hs.to_str().unwrap()], &work);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("not supported"), "{}", out.stderr);
}

/// The output of a tool run on `path` (it must succeed).
fn tool_output(tool: &str, args: &[&str], path: &std::path::Path) -> String {
    let out = Command::new(tool).args(args).arg(path).output().unwrap();
    assert!(out.status.success(), "{tool} {args:?} {}", path.display());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The runtime reaches its own functions and statics directly, not
/// through the GOT (FEAT-1026): no GOT slot of a program holds the address
/// of a static or of a C entry point (`cirka_*`) of the runtime's own
/// object. (The archive also holds `core`, `compiler_builtins` and
/// `cirka_bigint`, which reach the runtime and each other through the
/// GOT; the program's own functions, such as `cirka_call_1`, are reached
/// from the runtime through the GOT too.)
#[test]
fn runtime_symbols_are_not_reached_through_the_got() {
    use cirka_driver::native::{RuntimeKind, find_runtime};
    use std::collections::{HashMap, HashSet};
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let rt = find_runtime(&cirka_driver::Config::from_env(), RuntimeKind::Release).unwrap();
    let (_, cirka) = cirka_integration::cli_bins();
    let work = cirka_testkit::work_dir("driver-got", "net_recv_empty");
    let exe = work.join("net_recv_empty");
    let src = repo_root().join("tests/cirkalang/run/v007/net_recv_empty.cirka");
    let bd = format!("--build-dir={}", work.display());
    let args = [
        "build",
        "-O2",
        &bd,
        "-o",
        exe.to_str().unwrap(),
        src.to_str().unwrap(),
    ];
    let out = run(cirka, &args, &work);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    // The statics and C entry points of the runtime's object
    // (`archive:member:address type name`).
    let members = tool_output("nm", &["-A", "--defined-only"], &rt);
    let own: HashSet<&str> = members
        .lines()
        .filter_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [file, ty, name] => {
                let member = file.rsplit(':').nth(1)?;
                let data = "bBdDrR".contains(ty);
                let entry = ty == "T" && name.starts_with("cirka_");
                (member.starts_with("cirka_rt-") && (data || entry)).then_some(name)
            }
            _ => None,
        })
        .collect();
    assert!(own.contains("cirka_alloc"), "{members}");
    // The program's symbols by address.
    let syms = tool_output("nm", &["--defined-only"], &exe);
    let mut at: HashMap<u64, Vec<&str>> = HashMap::new();
    for l in syms.lines() {
        if let [a, _, name] = l.split_whitespace().collect::<Vec<_>>()[..] {
            at.entry(u64::from_str_radix(a, 16).unwrap())
                .or_default()
                .push(name);
        }
    }
    // The GOT, and the targets of its slots (`R_X86_64_RELATIVE`: the
    // addend is the address).
    let sections = tool_output("readelf", &["-SW"], &exe);
    let (got, size) = sections
        .lines()
        .find_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let i = f.iter().position(|w| *w == ".got")?;
            let hex = |w: &str| u64::from_str_radix(w, 16).ok();
            Some((hex(f.get(i + 2)?)?, hex(f.get(i + 4)?)?))
        })
        .unwrap_or_else(|| panic!("no .got:\n{sections}"));
    let relocs = tool_output("readelf", &["-rW"], &exe);
    let mut through_got = Vec::new();
    for l in relocs.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() != 4 || f[2] != "R_X86_64_RELATIVE" {
            continue;
        }
        let (Ok(off), Ok(target)) = (u64::from_str_radix(f[0], 16), u64::from_str_radix(f[3], 16))
        else {
            continue;
        };
        if (got..got + size).contains(&off) {
            for name in at.get(&target).into_iter().flatten() {
                if own.contains(name) {
                    through_got.push(*name);
                }
            }
        }
    }
    assert!(
        through_got.is_empty(),
        "reached through the GOT: {through_got:?}"
    );
}

/// The runtime functions that run for each operation of a program are
/// linked next to each other, with the program's closure calls
/// (`.text.hot.*`, FEAT-1023): between the first and the last of those a
/// server uses there is no other function, of the runtime or of the
/// program.
#[test]
fn hot_runtime_functions_are_linked_together() {
    cirka_testkit::init();
    cirka_driver::native::ensure_runtime_built().unwrap();
    let (_, cirka) = cirka_integration::cli_bins();
    let work = cirka_testkit::work_dir("driver-hot", "net_recv_empty");
    let exe = work.join("net_recv_empty");
    let src = repo_root().join("tests/cirkalang/run/v007/net_recv_empty.cirka");
    let bd = format!("--build-dir={}", work.display());
    let args = [
        "build",
        "-O2",
        &bd,
        "-o",
        exe.to_str().unwrap(),
        src.to_str().unwrap(),
    ];
    let out = run(cirka, &args, &work);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let nm = Command::new("nm")
        .args(["-n", "--defined-only"])
        .arg(&exe)
        .output()
        .unwrap();
    assert!(nm.status.success());
    // The functions of the text, in address order.
    let text = String::from_utf8_lossy(&nm.stdout);
    let syms: Vec<&str> = text
        .lines()
        .filter_map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [_, "T" | "t", name] => Some(name),
            _ => None,
        })
        .collect();
    let used = [
        "cirka_alloc",
        "cirka_free_object",
        "cirka_errno",
        "cirka_loop_run",
        "cirka_async_watch",
        "cirka_async_events",
        "cirka_sock_recv",
        "cirka_call_1",
    ];
    let at = |name: &str| {
        syms.iter()
            .position(|s| *s == name)
            .unwrap_or_else(|| panic!("{name} is not linked:\n{text}"))
    };
    let lo = used.iter().map(|n| at(n)).min().unwrap();
    let hi = used.iter().map(|n| at(n)).max().unwrap();
    let hot = |s: &str| {
        s.starts_with("cirka_apply_") && !s.ends_with("_slow")
            || s.contains("read_now")
            || s.contains("free_walk")
            || [
                "cirka_alloc",
                "cirka_free_object",
                "cirka_free_raw",
                "cirka_drop_reuse",
                "cirka_dec_ref_slow",
                "cirka_dup",
                "cirka_drop",
                "cirka_force",
                "cirka_mk_pap",
                "cirka_errno",
                "cirka_loop_run",
                "cirka_async_watch",
                "cirka_async_events",
                "cirka_sock_recv",
                "cirka_sock_send",
                "cirka_net_send",
                "cirka_sock_accept",
                "cirka_bytes_from",
                "cirka_call_1",
            ]
            .contains(&s)
    };
    let others: Vec<&str> = syms[lo..=hi].iter().copied().filter(|s| !hot(s)).collect();
    assert!(
        others.is_empty(),
        "{others:?} among the hot functions:\n{text}"
    );
}
