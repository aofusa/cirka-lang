//! Native code generation: LLVM IR → executable with `clang` and
//! `libcirka_rt.a` (07-backend.md §4).

use crate::Config;
use cirka_mir::ir::Module;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which runtime flavour to link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RuntimeKind {
    #[default]
    Release,
    /// Checks, poisoning and leak reports (`--runtime=debug`).
    Debug,
    /// The release runtime and the generated code instrumented for
    /// ThreadSanitizer (`--runtime=tsan`, 05-runtime.md §9.7).
    Tsan,
}

impl RuntimeKind {
    /// The code generation options for this runtime.
    pub fn codegen_options(self) -> cirka_codegen_llvm::Options {
        cirka_codegen_llvm::Options {
            debug_rt: self == RuntimeKind::Debug,
            tsan: self == RuntimeKind::Tsan,
        }
    }

    /// Where `scripts/build-runtime.sh` puts this flavour, and its
    /// subdirectory of `CIRKA_RUNTIME_DIR`.
    fn dirs(self) -> (&'static str, &'static str) {
        match self {
            RuntimeKind::Release => ("target/rt", ""),
            RuntimeKind::Debug => ("target/rt-debug", "debug"),
            RuntimeKind::Tsan => ("target/rt-tsan", "tsan"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NativeOptions {
    /// LLVM optimization level (0..=3).
    pub opt_level: u8,
    pub runtime: RuntimeKind,
    /// Extra libraries / linker arguments (FFI).
    pub link_args: Vec<String>,
}

impl Default for NativeOptions {
    fn default() -> Self {
        NativeOptions {
            opt_level: 2,
            runtime: RuntimeKind::Release,
            link_args: Vec::new(),
        }
    }
}

/// Locates `libcirka_rt.a` for the requested flavour.
///
/// Order: `$CIRKA_RUNTIME_DIR` (`libcirka_rt.a`, `debug/libcirka_rt.a`),
/// then the workspace build directories produced by
/// `scripts/build-runtime.sh` (`target/rt/release`, `target/rt-debug/release`)
/// found from the executable's location or the source tree.
pub fn find_runtime(cfg: &Config, kind: RuntimeKind) -> Result<PathBuf, String> {
    let (target, sub) = kind.dirs();
    if let Some(dir) = &cfg.runtime_dir {
        let p = dir.join(sub).join("libcirka_rt.a");
        return if p.exists() {
            Ok(p)
        } else {
            Err(format!("runtime library not found: {}", p.display()))
        };
    }
    let rel = format!("{target}/release/libcirka_rt.a");
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().map(Path::to_path_buf));
    }
    roots.push(workspace_root());
    for r in roots {
        let p = r.join(&rel);
        if p.exists() {
            return Ok(p);
        }
    }
    Err(format!(
        "runtime library `{rel}` not found; run scripts/build-runtime.sh or set CIRKA_RUNTIME_DIR"
    ))
}

/// The workspace root at build time of the driver.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Builds the release and debug runtime libraries if they are missing
/// (used by tests; the CI script builds them up front).
pub fn ensure_runtime_built() -> Result<(), String> {
    static ONCE: std::sync::Mutex<Option<Result<(), String>>> = std::sync::Mutex::new(None);
    let mut g = ONCE.lock().unwrap();
    if let Some(r) = &*g {
        return r.clone();
    }
    let r = build_runtime(RuntimeKind::Release).and_then(|()| build_runtime(RuntimeKind::Debug));
    *g = Some(r.clone());
    r
}

/// Builds the ThreadSanitizer runtime if it is missing (tests with threads).
pub fn ensure_tsan_runtime_built() -> Result<(), String> {
    static ONCE: std::sync::Mutex<Option<Result<(), String>>> = std::sync::Mutex::new(None);
    let mut g = ONCE.lock().unwrap();
    if let Some(r) = &*g {
        return r.clone();
    }
    let r = build_runtime(RuntimeKind::Tsan);
    *g = Some(r.clone());
    r
}

/// Whether ThreadSanitizer programs can run here: a probe built with
/// `-fsanitize=thread` runs. With high-entropy ASLR it re-executes itself
/// with ASLR disabled through `personality(2)`, which Docker's default
/// seccomp profile forbids (05-runtime.md §9.7): the tests skip the tsan
/// configurations there, and `scripts/ci.sh` says so.
pub fn tsan_usable(cfg: &Config) -> bool {
    static USABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *USABLE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("cirka-tsan-probe-{}", std::process::id()));
        if std::fs::create_dir_all(&dir).is_err() {
            return false;
        }
        let (c, exe) = (dir.join("probe.c"), dir.join("probe"));
        let ok = std::fs::write(&c, "int main(void) { return 0; }\n").is_ok()
            && Command::new(&cfg.clang)
                .arg("-fsanitize=thread")
                .arg(&c)
                .arg("-o")
                .arg(&exe)
                .output()
                .is_ok_and(|o| o.status.success())
            && Command::new(&exe)
                .output()
                .is_ok_and(|o| o.status.success());
        let _ = std::fs::remove_dir_all(&dir);
        ok
    })
}

/// The `RUSTFLAGS` of every flavour of the runtime: code of an executable,
/// whose own calls and statics need no GOT (FEAT-1026).
const RUNTIME_RUSTFLAGS: &str = "-C relocation-model=pie";

/// The further `RUSTFLAGS` of the ThreadSanitizer flavour: stable Rust has
/// no sanitizers (built with `RUSTC_BOOTSTRAP=1`), `core` (not
/// instrumented) is accepted with `-Cunsafe-allow-abi-mismatch=sanitizer`
/// (05-runtime.md §9.7), and the runtime calls libc's I/O functions
/// (`--cfg cirka_tsan`, FEAT-1007).
const TSAN_RUSTFLAGS: &str =
    "-Zsanitizer=thread -Cunsafe-allow-abi-mismatch=sanitizer --cfg cirka_tsan";

/// Builds one flavour of the runtime as `scripts/build-runtime.sh` does
/// (the same flags: cargo rebuilds the runtime when they change).
fn build_runtime(kind: RuntimeKind) -> Result<(), String> {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(workspace_root()).args([
        "build",
        "-q",
        "-p",
        "cirka-runtime",
        "--release",
        "--target-dir",
        kind.dirs().0,
    ]);
    match kind {
        RuntimeKind::Release => {
            cmd.env("RUSTFLAGS", RUNTIME_RUSTFLAGS);
        }
        RuntimeKind::Debug => {
            cmd.env("RUSTFLAGS", RUNTIME_RUSTFLAGS)
                .args(["--features", "debug-rt"]);
        }
        RuntimeKind::Tsan => {
            cmd.env("RUSTC_BOOTSTRAP", "1")
                .env("RUSTFLAGS", format!("{RUNTIME_RUSTFLAGS} {TSAN_RUSTFLAGS}"));
        }
    }
    match cmd.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "building the runtime failed:\n{}",
            String::from_utf8_lossy(&o.stderr)
        )),
        Err(e) => Err(format!("cannot run cargo: {e}")),
    }
}

/// The LLVM optimization level of an optimization level of the compiler:
/// `-O2` compares with `opt-level=3` of a Rust release build (07-backend.md).
pub fn llvm_opt_level(opt_level: u8) -> u8 {
    match opt_level {
        0 | 1 => opt_level,
        _ => 3,
    }
}

/// The optimization arguments of `clang` for `opt_level`: `-O<n>`, and at
/// LLVM `-O3` the vectorization of floating-point reductions that keeps
/// the order of the additions (FEAT-1010, 07-backend.md §3): the
/// additions are done lane by lane in the original order, so the result
/// does not change, and the other operations of the loop (divisions,
/// multiplications, conversions) are done several at a time.
pub fn llvm_opt_args(opt_level: u8) -> Vec<String> {
    let l = llvm_opt_level(opt_level);
    let mut args = vec![format!("-O{l}")];
    if l >= 3 {
        args.extend([
            "-mllvm".to_string(),
            "-force-ordered-reductions".to_string(),
        ]);
    }
    args
}

/// Compiles LLVM IR text into an executable.
pub fn link_ir(
    cfg: &Config,
    ir: &str,
    work: &Path,
    exe: &Path,
    opts: &NativeOptions,
) -> Result<(), String> {
    std::fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    let ll = work.join("out.ll");
    std::fs::write(&ll, ir).map_err(|e| format!("cannot write {}: {e}", ll.display()))?;
    let rt = find_runtime(cfg, opts.runtime)?;
    let mut cmd = Command::new(&cfg.clang);
    cmd.args(llvm_opt_args(opts.opt_level))
        .arg(format!("--target={}", cfg.target))
        .arg("-Wno-override-module");
    if opts.runtime == RuntimeKind::Tsan {
        // Runs the instrumentation pass on the `sanitize_thread` functions
        // and links the ThreadSanitizer runtime.
        cmd.arg("-fsanitize=thread");
    }
    // libm comes before the runtime: the archive also holds the weak math
    // functions of Rust's compiler-builtins (`cbrt`, `fma`, `floor`, ...),
    // which would otherwise be taken for the program's calls instead of the
    // C library's (BUG-0028).
    let out = cmd
        .arg(&ll)
        .arg("-lm")
        .arg(&rt)
        .args(&opts.link_args)
        .args(["-lm", "-lpthread", "-Wl,--gc-sections", "-o"])
        .arg(exe)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", cfg.clang))?;
    if !out.status.success() {
        return Err(format!(
            "{} failed:\n{}",
            cfg.clang,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Verifies, generates code for and links a MIR program.
pub fn build_mir(
    cfg: &Config,
    m: &Module,
    work: &Path,
    exe: &Path,
    opts: &NativeOptions,
) -> Result<String, String> {
    let errs = cirka_mir::verify::verify_module(m);
    if !errs.is_empty() {
        return Err(format!(
            "internal compiler error: MIR verification failed\n{}",
            cirka_mir::verify::render_errors(&errs)
        ));
    }
    let ir = cirka_codegen_llvm::emit(m, opts.runtime.codegen_options());
    let mut opts = opts.clone();
    opts.link_args.extend(lib_args(&m.libs));
    link_ir(cfg, &ir, work, exe, &opts)?;
    Ok(ir)
}

/// The linker arguments of the C libraries of a program (`-l<lib>`).
pub fn lib_args(libs: &[String]) -> Vec<String> {
    libs.iter().map(|l| format!("-l{l}")).collect()
}

/// Runs `opt -passes=verify` on IR text (tests).
pub fn verify_ir(cfg: &Config, ir_path: &Path) -> Result<(), String> {
    let out = Command::new(&cfg.opt)
        .args(["-passes=verify", "-disable-output"])
        .arg(ir_path)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", cfg.opt))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{RUNTIME_RUSTFLAGS, TSAN_RUSTFLAGS, llvm_opt_args, llvm_opt_level, workspace_root};

    /// The driver builds the runtime with the flags of
    /// scripts/build-runtime.sh: otherwise each would rebuild the runtime
    /// the other built, with other code (FEAT-1026).
    #[test]
    fn runtime_flags_are_those_of_the_build_script() {
        let script =
            std::fs::read_to_string(workspace_root().join("scripts/build-runtime.sh")).unwrap();
        assert!(
            script.contains(&format!("pie=\"{RUNTIME_RUSTFLAGS}\"")),
            "{script}"
        );
        assert!(
            script.contains(&format!("RUSTFLAGS=\"$pie {TSAN_RUSTFLAGS}\"")),
            "{script}"
        );
        assert_eq!(script.matches("RUSTFLAGS=\"$pie").count(), 3, "{script}");
    }

    #[test]
    fn release_builds_use_llvm_o3() {
        assert_eq!(llvm_opt_level(0), 0);
        assert_eq!(llvm_opt_level(1), 1);
        assert_eq!(llvm_opt_level(2), 3);
        assert_eq!(llvm_opt_level(3), 3);
    }

    #[test]
    fn release_builds_vectorize_ordered_reductions() {
        assert_eq!(llvm_opt_args(0), ["-O0"]);
        assert_eq!(llvm_opt_args(1), ["-O1"]);
        for l in [2, 3] {
            assert_eq!(
                llvm_opt_args(l),
                ["-O3", "-mllvm", "-force-ordered-reductions"]
            );
        }
    }
}
