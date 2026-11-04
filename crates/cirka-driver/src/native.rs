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
    if let Some(dir) = &cfg.runtime_dir {
        let p = match kind {
            RuntimeKind::Release => dir.join("libcirka_rt.a"),
            RuntimeKind::Debug => dir.join("debug").join("libcirka_rt.a"),
        };
        return if p.exists() {
            Ok(p)
        } else {
            Err(format!("runtime library not found: {}", p.display()))
        };
    }
    let rel = match kind {
        RuntimeKind::Release => "target/rt/release/libcirka_rt.a",
        RuntimeKind::Debug => "target/rt-debug/release/libcirka_rt.a",
    };
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().map(Path::to_path_buf));
    }
    roots.push(workspace_root());
    for r in roots {
        let p = r.join(rel);
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

/// Builds the runtime libraries if they are missing (used by tests; the CI
/// script builds them up front).
pub fn ensure_runtime_built() -> Result<(), String> {
    static ONCE: std::sync::Mutex<Option<Result<(), String>>> = std::sync::Mutex::new(None);
    let mut g = ONCE.lock().unwrap();
    if let Some(r) = &*g {
        return r.clone();
    }
    let root = workspace_root();
    let mut r = Ok(());
    for (dir, feat) in [("target/rt", None), ("target/rt-debug", Some("debug-rt"))] {
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.current_dir(&root).args([
            "build",
            "-q",
            "-p",
            "cirka-runtime",
            "--release",
            "--target-dir",
            dir,
        ]);
        if let Some(f) = feat {
            cmd.args(["--features", f]);
        }
        match cmd.output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                r = Err(format!(
                    "building the runtime failed:\n{}",
                    String::from_utf8_lossy(&o.stderr)
                ));
                break;
            }
            Err(e) => {
                r = Err(format!("cannot run cargo: {e}"));
                break;
            }
        }
    }
    *g = Some(r.clone());
    r
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
    let out = Command::new(&cfg.clang)
        .arg(format!("-O{}", opts.opt_level.min(3)))
        .arg(format!("--target={}", cfg.target))
        .arg("-Wno-override-module")
        .arg(&ll)
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
    let ir = cirka_codegen_llvm::emit(
        m,
        cirka_codegen_llvm::Options {
            debug_rt: opts.runtime == RuntimeKind::Debug,
        },
    );
    link_ir(cfg, &ir, work, exe, opts)?;
    Ok(ir)
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
