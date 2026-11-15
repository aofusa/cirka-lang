//! cirka-bindgen: C header import (FEAT-0048, 08-ffi.md §6). libclang
//! reads the header into language-neutral declarations ([`model`]), which
//! the printers write as aCHC (Haskell 2010 FFI and `Storable`) or
//! CirkaLang (`extern` blocks). `achc bindgen` and `cirka bindgen` are
//! [`cli::main`].

#[allow(unsafe_code)] // libclang, the crate's FFI boundary (see the module comment).
pub mod clang;
pub mod cli;
pub mod model;
pub mod print_cirka;
pub mod print_hs;
mod read;

use model::{Def, Globs, Header, Item, reserved};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// What to read and what to output.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub header: PathBuf,
    /// Arguments for clang (`-I`, `-D`, …).
    pub clang_args: Vec<String>,
    /// Names of the declarations to output (with what they refer to).
    pub allowlist: Option<Globs>,
    /// Paths of the files whose declarations to output.
    pub files: Option<Globs>,
    /// Functions declared without `IO`.
    pub pure: Option<Globs>,
    /// Libraries to link.
    pub links: Vec<String>,
    pub module: Option<String>,
}

/// The external tools, from the environment (`.env`).
#[derive(Clone, Debug)]
pub struct Env {
    /// `CIRKA_LIBCLANG` (empty: search).
    pub libclang: String,
    /// `CIRKA_LLVM_VERSION`.
    pub llvm_version: String,
    /// `CIRKA_CLANG`, asked for its resource directory.
    pub clang: String,
}

impl Env {
    pub fn from_env() -> Env {
        let var = |k: &str, d: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| d.to_string())
        };
        let llvm_version = var("CIRKA_LLVM_VERSION", "18");
        Env {
            libclang: var("CIRKA_LIBCLANG", ""),
            clang: var("CIRKA_CLANG", &format!("clang-{llvm_version}")),
            llvm_version,
        }
    }

    /// clang's resource directory (its built-in headers).
    fn resource_dir(&self) -> Option<String> {
        let out = std::process::Command::new(&self.clang)
            .arg("-print-resource-dir")
            .output()
            .ok()?;
        let dir = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (out.status.success() && !dir.is_empty()).then_some(dir)
    }
}

/// Reads the header of `opts` and selects the declarations to output.
pub fn generate(env: &Env, opts: &Options) -> Result<Header, Vec<String>> {
    // libclang is used from one thread at a time.
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !opts.header.is_file() {
        return Err(vec![format!(
            "error: cannot read {}",
            opts.header.display()
        )]);
    }
    let clang = clang::load(Some(&env.libclang), &env.llvm_version)
        .map_err(|e| vec![format!("error: {e}")])?;
    let resource_dir = env.resource_dir();
    let mut args: Vec<String> = vec![
        "-x".into(),
        "c".into(),
        "-std=gnu17".into(),
        "-ferror-limit=0".into(),
    ];
    if let Some(d) = &resource_dir {
        args.push("-resource-dir".into());
        args.push(d.clone());
    }
    args.extend(opts.clang_args.iter().cloned());
    let (items, files) = read::read(clang, &opts.header, &args, resource_dir.as_deref())?;
    Ok(select(items, &files, opts))
}

/// The roots (the declarations matching the filters, or all without
/// filters) and what they refer to, in source order.
fn select(items: Vec<Item>, files: &[String], opts: &Options) -> Header {
    let index: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (it.key.as_str(), i))
        .collect();
    let root = |it: &Item| {
        let name = opts.allowlist.as_ref().is_none_or(|a| {
            a.matches(&it.name)
                || matches!(&it.def, Def::Enum(e) if e.consts.iter().any(|(n, _)| a.matches(n)))
        });
        let file = opts.files.as_ref().is_none_or(|f| f.matches(&it.file));
        !reserved(&it.name) && name && file
    };
    let mut keep = vec![false; items.len()];
    let mut stack: Vec<usize> = (0..items.len()).filter(|&i| root(&items[i])).collect();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut keep[i], true) {
            continue;
        }
        let mut refs = Vec::new();
        match &items[i].def {
            Def::Record(r) => r.fields.iter().for_each(|f| f.ty.refs(&mut refs)),
            Def::Alias(t) => t.refs(&mut refs),
            Def::Const(c) => c.ty.refs(&mut refs),
            Def::Function(f) => {
                f.sig.ret.refs(&mut refs);
                f.sig.params.iter().for_each(|p| p.refs(&mut refs));
            }
            Def::Enum(_) | Def::Opaque(_) | Def::Skipped(_) => {}
        }
        stack.extend(refs.iter().filter_map(|k| index.get(k.as_str())));
    }
    let rank = |f: &str| files.iter().position(|g| g == f).unwrap_or(usize::MAX);
    let mut out: Vec<Item> = items
        .into_iter()
        .zip(keep)
        .filter_map(|(it, k)| k.then_some(it))
        .collect();
    out.sort_by_key(|it| (rank(&it.file), it.line));
    Header::new(out)
}

/// The output language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// aCHC: Haskell 2010 FFI and `Storable`.
    Haskell,
    /// CirkaLang: `extern` blocks.
    Cirka,
}

/// Writes the declarations as source code of `lang`. `source` names the
/// header in the heading comment.
pub fn print(h: &Header, opts: &Options, lang: Lang, source: &str) -> String {
    match lang {
        Lang::Haskell => print_hs::print(h, opts, source),
        Lang::Cirka => print_cirka::print(h, opts, source),
    }
}
