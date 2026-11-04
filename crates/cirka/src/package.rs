//! `cirka.toml`: the package description (FEAT-0205).
//!
//! ```toml
//! [package]
//! name = "hello"
//! edition = "0.0.1"
//! ```
//!
//! The sources are in `src/`; the main module is `src/Main.cirka`.
//! Dependencies are not supported in v0.0.1. Only the subset of TOML used
//! above is accepted (sections, `key = "string"`, comments).

use crate::EDITION;
use cirka_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub root: PathBuf,
    pub name: String,
    pub edition: String,
}

impl Package {
    /// Loads `dir/cirka.toml` (or the given `cirka.toml`).
    pub fn load(path: &Path) -> Result<Package, Diagnostic> {
        let file = if path.is_dir() {
            path.join("cirka.toml")
        } else {
            path.to_path_buf()
        };
        let root = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let text = std::fs::read_to_string(&file).map_err(|e| {
            Diagnostic::error(format!("cannot read {}: {e}", file.display())).with_code("E1104")
        })?;
        Package::parse(&text, root)
            .map_err(|m| Diagnostic::error(format!("{}: {m}", file.display())).with_code("E1001"))
    }

    pub fn parse(text: &str, root: PathBuf) -> Result<Package, String> {
        let mut section = String::new();
        let mut name = None;
        let mut edition = None;
        for (i, line) in text.lines().enumerate() {
            let l = line.split('#').next().unwrap_or("").trim();
            if l.is_empty() {
                continue;
            }
            if let Some(s) = l.strip_prefix('[') {
                section = s
                    .strip_suffix(']')
                    .ok_or(format!("line {}: bad section header", i + 1))?
                    .trim()
                    .to_string();
                continue;
            }
            let (k, v) = l
                .split_once('=')
                .ok_or(format!("line {}: expected `key = \"value\"`", i + 1))?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or(format!("line {}: the value must be a string", i + 1))?;
            match (section.as_str(), k.trim()) {
                ("package", "name") => name = Some(v.to_string()),
                ("package", "edition") => edition = Some(v.to_string()),
                ("package", k) => return Err(format!("line {}: unknown key `{k}`", i + 1)),
                ("dependencies", _) => {
                    return Err(format!(
                        "line {}: dependencies are not supported in v{EDITION}",
                        i + 1
                    ));
                }
                (s, _) => return Err(format!("line {}: unknown section `[{s}]`", i + 1)),
            }
        }
        let name = name.ok_or("missing `name` in `[package]`")?;
        let edition = edition.ok_or("missing `edition` in `[package]`")?;
        if edition != EDITION {
            return Err(format!(
                "edition `{edition}` is not supported (this compiler implements `{EDITION}`)"
            ));
        }
        Ok(Package {
            root,
            name,
            edition,
        })
    }

    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn main_file(&self) -> PathBuf {
        self.src_dir().join("Main.cirka")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package() {
        let p = Package::parse(
            "# a package\n[package]\nname = \"hello\"\nedition = \"0.0.1\"\n",
            PathBuf::from("x"),
        )
        .unwrap();
        assert_eq!(p.name, "hello");
        assert_eq!(p.main_file(), PathBuf::from("x/src/Main.cirka"));
        assert!(
            Package::parse(
                "[package]\nname = \"a\"\nedition = \"9.9.9\"\n",
                PathBuf::new()
            )
            .unwrap_err()
            .contains("not supported")
        );
        assert!(
            Package::parse("[dependencies]\nfoo = \"1\"\n", PathBuf::new())
                .unwrap_err()
                .contains("dependencies")
        );
        assert!(
            Package::parse("[package]\nedition = \"0.0.1\"\n", PathBuf::new())
                .unwrap_err()
                .contains("name")
        );
    }
}
