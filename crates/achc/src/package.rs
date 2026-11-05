//! `achc.toml`: the package description (01-architecture.md §5, FEAT-0105).
//!
//! ```toml
//! [package]
//! name = "hello"
//! main = "src/Main.hs"        # default: src/Main.hs
//! source-dirs = ["src"]       # default: ["src"]
//!
//! [compiler]
//! ownership = "warn"          # off | warn | strict (.hs files, 02 §1.3)
//! cycles = "warn"             # warn | error | collect
//! default-extensions = []
//! ```
//!
//! The base library is always available; other dependencies are not
//! supported yet. Only the subset of TOML used above is accepted
//! (sections, strings, arrays of strings, comments).

use cirka_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub root: PathBuf,
    pub name: String,
    pub main: String,
    pub source_dirs: Vec<String>,
    pub ownership: String,
    pub cycles: String,
    pub default_extensions: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Value {
    Str(String),
    List(Vec<String>),
}

fn parse_value(v: &str) -> Option<Value> {
    let v = v.trim();
    if let Some(s) = v.strip_prefix('"') {
        return s.strip_suffix('"').map(|s| Value::Str(s.to_string()));
    }
    let inner = v.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Value::List(Vec::new()));
    }
    inner
        .split(',')
        .map(|x| {
            let x = x.trim();
            x.strip_prefix('"')
                .and_then(|x| x.strip_suffix('"'))
                .map(str::to_string)
        })
        .collect::<Option<Vec<String>>>()
        .map(Value::List)
}

impl Package {
    /// Loads `dir/achc.toml` (or the given `achc.toml`).
    pub fn load(path: &Path) -> Result<Package, Diagnostic> {
        let file = if path.is_dir() {
            path.join("achc.toml")
        } else {
            path.to_path_buf()
        };
        let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let text = std::fs::read_to_string(&file).map_err(|e| {
            Diagnostic::error(format!("cannot read {}: {e}", file.display())).with_code("E0002")
        })?;
        Package::parse(&text, root)
            .map_err(|m| Diagnostic::error(format!("{}: {m}", file.display())).with_code("E0002"))
    }

    pub fn parse(text: &str, root: PathBuf) -> Result<Package, String> {
        let mut section = String::new();
        let mut p = Package {
            root,
            name: String::new(),
            main: "src/Main.hs".into(),
            source_dirs: vec!["src".into()],
            ownership: "warn".into(),
            cycles: "warn".into(),
            default_extensions: Vec::new(),
        };
        let mut has_name = false;
        for (i, line) in text.lines().enumerate() {
            let l = line.split('#').next().unwrap_or("").trim();
            if l.is_empty() {
                continue;
            }
            let at = |m: String| format!("line {}: {m}", i + 1);
            if let Some(s) = l.strip_prefix('[') {
                section = s
                    .strip_suffix(']')
                    .ok_or(at("bad section header".into()))?
                    .trim()
                    .to_string();
                if !matches!(section.as_str(), "package" | "compiler" | "dependencies") {
                    return Err(at(format!("unknown section `[{section}]`")));
                }
                continue;
            }
            let (k, v) = l
                .split_once('=')
                .ok_or(at("expected `key = value`".into()))?;
            let k = k.trim();
            let v = parse_value(v).ok_or(at(
                "the value must be a string or an array of strings".into(),
            ))?;
            let choice = |v: Value, allowed: &[&str]| match v {
                Value::Str(s) if allowed.contains(&s.as_str()) => Ok(s),
                _ => Err(at(format!(
                    "`{k}` must be one of {}",
                    allowed
                        .iter()
                        .map(|a| format!("\"{a}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))),
            };
            match (section.as_str(), k, v) {
                ("package", "name", Value::Str(s)) => {
                    p.name = s;
                    has_name = true;
                }
                ("package", "main", Value::Str(s)) => p.main = s,
                ("package", "source-dirs", Value::List(l)) => p.source_dirs = l,
                ("compiler", "ownership", v) => {
                    p.ownership = choice(v, &["off", "warn", "strict"])?
                }
                ("compiler", "cycles", v) => p.cycles = choice(v, &["warn", "error", "collect"])?,
                ("compiler", "default-extensions", Value::List(l)) => p.default_extensions = l,
                ("dependencies", "cirka-base", _) => {}
                ("dependencies", k, _) => {
                    return Err(at(format!(
                        "dependency `{k}`: only the base library is supported yet"
                    )));
                }
                (s, k, _) => return Err(at(format!("unknown or ill-typed key `{k}` in `[{s}]`"))),
            }
        }
        if !has_name {
            return Err("missing `name` in `[package]`".into());
        }
        if !p.default_extensions.is_empty() {
            return Err("`default-extensions` is not supported yet; use LANGUAGE pragmas".into());
        }
        Ok(p)
    }

    pub fn main_file(&self) -> PathBuf {
        self.root.join(&self.main)
    }

    pub fn source_dirs(&self) -> Vec<PathBuf> {
        self.source_dirs.iter().map(|d| self.root.join(d)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package() {
        let p = Package::parse(
            "# a package\n[package]\nname = \"hello\"\nsource-dirs = [\"src\", \"lib\"]\n\
             [compiler]\nownership = \"strict\"\n[dependencies]\ncirka-base = \"*\"\n",
            PathBuf::from("x"),
        )
        .unwrap();
        assert_eq!(p.name, "hello");
        assert_eq!(p.main_file(), PathBuf::from("x/src/Main.hs"));
        assert_eq!(
            p.source_dirs(),
            vec![PathBuf::from("x/src"), PathBuf::from("x/lib")]
        );
        assert_eq!(p.ownership, "strict");
        let err = |t: &str| Package::parse(t, PathBuf::new()).unwrap_err();
        assert!(err("[package]\nmain = \"a.hs\"\n").contains("name"));
        assert!(
            err("[package]\nname = \"a\"\n[compiler]\ncycles = \"never\"\n").contains("one of")
        );
        assert!(err("[package]\nname = \"a\"\n[dependencies]\nfoo = \"1\"\n").contains("base"));
        assert!(err("[other]\n").contains("unknown section"));
    }
}
