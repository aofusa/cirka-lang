//! `achc bindgen` / `cirka bindgen` (08-ffi.md §6).

use crate::model::Globs;
use crate::{Env, Lang, Options, generate, print};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: TOOL bindgen [options] HEADER.h [-o FILE] [-- CLANG-ARGS...]
  --allowlist=PATTERNS  names of the declarations to output, with the types
                        they use (`|`-separated globs, `*` any text)
  --file=PATTERNS       paths of the files whose declarations to output
  --pure=PATTERNS       functions declared without IO
  --link=LIB            a library to link
  --module=NAME         the module name
  --extern              aCHC: an `extern \"C\"` block instead of `foreign import`s
                        (CirkaLang always writes an extern block)
  -I DIR  -D NAME[=V]  -U NAME   passed to clang";

/// The arguments of `bindgen`.
#[derive(Debug)]
pub struct Args {
    pub opts: Options,
    pub output: Option<PathBuf>,
}

/// Parses the arguments after `bindgen`.
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut opts = Options::default();
    let mut output = None;
    let mut header = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = |p: &str| a.strip_prefix(p).map(str::to_string);
        match a.as_str() {
            "-o" => output = Some(PathBuf::from(it.next().ok_or("`-o` needs a file name")?)),
            "--" => opts.clang_args.extend(it.by_ref().cloned()),
            "-I" | "-D" | "-U" | "-include" | "-isystem" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("`{a}` needs an argument"))?;
                opts.clang_args.push(a.clone());
                opts.clang_args.push(v.clone());
            }
            s if s.starts_with("-I") || s.starts_with("-D") || s.starts_with("-U") => {
                opts.clang_args.push(s.to_string())
            }
            s if s.starts_with("--allowlist=") => {
                opts.allowlist = value("--allowlist=").map(|v| Globs::parse(&v))
            }
            s if s.starts_with("--file=") => {
                opts.files = value("--file=").map(|v| Globs::parse(&v))
            }
            s if s.starts_with("--pure=") => opts.pure = value("--pure=").map(|v| Globs::parse(&v)),
            s if s.starts_with("--link=") => opts.links.extend(value("--link=")),
            s if s.starts_with("--module=") => opts.module = value("--module="),
            "--extern" => opts.extern_block = true,
            s if s.starts_with('-') => return Err(format!("unknown option `{s}`")),
            s => {
                if header.is_some() {
                    return Err(format!("unexpected argument `{s}`"));
                }
                header = Some(PathBuf::from(s));
            }
        }
    }
    opts.header = header.ok_or("no header file")?;
    Ok(Args { opts, output })
}

/// The Haskell module name for the output file `p` (`math.hs` → `Math`).
fn module_of(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_str()?;
    let s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let s = crate::print_hs::capitalized(&s);
    s.starts_with(|c: char| c.is_ascii_uppercase()).then_some(s)
}

/// `TOOL bindgen ARGS`: writes the declarations of the header in the
/// language of the tool.
pub fn main(tool: &str, args: &[String], lang: Lang) -> ExitCode {
    let mut a = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{tool} bindgen: {e}\n{}", USAGE.replace("TOOL", tool));
            return ExitCode::from(2);
        }
    };
    if lang == Lang::Haskell && a.opts.module.is_none() {
        a.opts.module = a.output.as_deref().and_then(module_of);
    }
    let h = match generate(&Env::from_env(), &a.opts) {
        Ok(h) => h,
        Err(es) => {
            for e in es {
                eprintln!("{e}");
            }
            eprintln!("{tool} bindgen: no declarations written");
            return ExitCode::from(1);
        }
    };
    let text = print(&h, &a.opts, lang, &a.opts.header.display().to_string());
    match &a.output {
        Some(p) => {
            if let Err(e) = std::fs::write(p, text) {
                eprintln!("{tool} bindgen: cannot write {}: {e}", p.display());
                return ExitCode::from(1);
            }
        }
        None => print!("{text}"),
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<Args, String> {
        parse_args(&s.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn arguments() {
        let a =
            args("--allowlist=vk* -I inc -DX=1 h.h -o out.hs --link=vulkan -- -std=c11").unwrap();
        assert_eq!(a.opts.header, PathBuf::from("h.h"));
        assert_eq!(a.output, Some(PathBuf::from("out.hs")));
        assert_eq!(a.opts.clang_args, ["-I", "inc", "-DX=1", "-std=c11"]);
        assert_eq!(a.opts.links, ["vulkan"]);
        assert!(a.opts.allowlist.unwrap().matches("vkCreateInstance"));
        assert!(!a.opts.extern_block);
        assert!(args("--extern h.h").unwrap().opts.extern_block);
        assert!(args("h.h g.h").is_err());
        assert!(args("--bogus h.h").is_err());
        assert!(args("").is_err());
        assert_eq!(
            module_of(Path::new("dir/vulkan-core.hs")).as_deref(),
            Some("Vulkan_core")
        );
    }
}
