//! `cirka.toml`: the package description and its dependencies (FEAT-0205,
//! FEAT-0214).
//!
//! ```toml
//! [package]
//! name = "app"
//! version = "0.1.0"
//! edition = "0.0.3"
//!
//! [dependencies]
//! util = { path = "../util" }
//! ```
//!
//! The sources are in `src/`; the main module of a program is
//! `src/Main.cirka`. Each package has a namespace, the name in PascalCase
//! (`math-tools` is `MathTools`). The modules of a dependency must be in its
//! namespace (`src/MathTools/Vec.cirka` is `MathTools.Vec`) and can be
//! imported only by the packages that depend on it. Only the subset of TOML
//! used above is accepted (sections, `key = "string"`,
//! `key = { path = "string" }`, comments).

use crate::EDITION;
use cirka_diagnostics::{Diagnostic, FileId, SourceMap};
use cirkalang_resolve::Loader;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub name: String,
    /// Relative to the package that declares it.
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub root: PathBuf,
    pub name: String,
    pub edition: String,
    pub dependencies: Vec<Dependency>,
}

/// The module namespace of a package named `name`, if it can have one.
pub fn namespace_of(name: &str) -> Option<String> {
    let mut out = String::new();
    for part in name.split(['-', '_']) {
        let mut cs = part.chars();
        let first = cs.next()?;
        if !first.is_ascii_alphabetic() {
            return None;
        }
        out.push(first.to_ascii_uppercase());
        for c in cs {
            if !c.is_ascii_alphanumeric() {
                return None;
            }
            out.push(c);
        }
    }
    Some(out)
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
        let mut dependencies: Vec<Dependency> = Vec::new();
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
            let (k, v) = (k.trim(), v.trim());
            if section == "dependencies" {
                if dependencies.iter().any(|d| d.name == k) {
                    return Err(format!("line {}: dependency `{k}` is repeated", i + 1));
                }
                let path = parse_dependency(v).ok_or(format!(
                    "line {}: expected `{k} = {{ path = \"..\" }}`",
                    i + 1
                ))?;
                dependencies.push(Dependency {
                    name: k.to_string(),
                    path: PathBuf::from(path),
                });
                continue;
            }
            let v = unquote(v).ok_or(format!("line {}: the value must be a string", i + 1))?;
            match (section.as_str(), k) {
                ("package", "name") => name = Some(v.to_string()),
                ("package", "edition") => edition = Some(v.to_string()),
                // Recorded for the package's users; not interpreted yet.
                ("package", "version") => {}
                ("package", k) => return Err(format!("line {}: unknown key `{k}`", i + 1)),
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
            dependencies,
        })
    }

    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn main_file(&self) -> PathBuf {
        self.src_dir().join("Main.cirka")
    }

    /// The modules in `src/`, by name, sorted.
    pub fn modules(&self) -> Vec<String> {
        fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let name = if prefix.is_empty() {
                    stem.to_string()
                } else {
                    format!("{prefix}.{stem}")
                };
                if p.is_dir() {
                    walk(&p, &name, out);
                } else if p.extension().is_some_and(|x| x == "cirka") {
                    out.push(name);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.src_dir(), "", &mut out);
        out.sort();
        out
    }
}

fn unquote(v: &str) -> Option<&str> {
    v.strip_prefix('"')?.strip_suffix('"')
}

/// The path in `{ path = "..." }`.
fn parse_dependency(v: &str) -> Option<&str> {
    let body = v.strip_prefix('{')?.strip_suffix('}')?.trim();
    let (k, p) = body.split_once('=')?;
    if k.trim() != "path" {
        return None;
    }
    unquote(p.trim())
}

/// A package with everything it depends on.
#[derive(Debug)]
pub struct Workspace {
    pub root: Package,
    /// The namespaces the root package may import.
    pub root_deps: Vec<String>,
    /// The dependencies by namespace.
    pub members: HashMap<String, Member>,
}

#[derive(Debug)]
pub struct Member {
    pub package: Package,
    /// The namespaces this package may import.
    pub deps: Vec<String>,
}

impl Workspace {
    /// Loads the package at `path` and its dependencies, transitively.
    pub fn load(path: &Path) -> Result<Workspace, Diagnostic> {
        let root = Package::load(path)?;
        let mut w = Workspace {
            root_deps: Vec::new(),
            root: root.clone(),
            members: HashMap::new(),
        };
        let mut chain = vec![canonical(&root.root)];
        let mut names = HashMap::new();
        w.root_deps = w.add_deps(&root, &mut chain, &mut names)?;
        Ok(w)
    }

    fn add_deps(
        &mut self,
        p: &Package,
        chain: &mut Vec<PathBuf>,
        where_from: &mut HashMap<String, PathBuf>,
    ) -> Result<Vec<String>, Diagnostic> {
        let err = |m: String| Diagnostic::error(m).with_code("E1118");
        let mut out = Vec::new();
        for d in &p.dependencies {
            let dir = p.root.join(&d.path);
            let at = canonical(&dir);
            if chain.contains(&at) {
                return Err(err(format!(
                    "dependency cycle through package `{}`",
                    d.name
                )));
            }
            let dep = Package::load(&dir).map_err(|e| {
                let m = e.message.clone();
                err(format!("dependency `{}` of `{}`: {m}", d.name, p.name))
            })?;
            if dep.name != d.name {
                return Err(err(format!(
                    "`{}` declares dependency `{}`, but the package at {} is named `{}`",
                    p.name,
                    d.name,
                    d.path.display(),
                    dep.name
                )));
            }
            let ns = namespace_of(&dep.name).ok_or_else(|| {
                err(format!(
                    "the package name `{}` cannot be a module namespace",
                    dep.name
                ))
            })?;
            match where_from.get(&ns) {
                Some(other) if *other != at => {
                    return Err(err(format!(
                        "two different packages are named `{}`: {} and {}",
                        dep.name,
                        other.display(),
                        at.display()
                    )));
                }
                Some(_) => {}
                None => {
                    where_from.insert(ns.clone(), at.clone());
                    chain.push(at);
                    let deps = self.add_deps(&dep, chain, where_from)?;
                    chain.pop();
                    self.members
                        .insert(ns.clone(), Member { package: dep, deps });
                }
            }
            out.push(ns);
        }
        Ok(out)
    }

    /// The package of module `module`, as a namespace (`None`: the root
    /// package or a library).
    fn owner(&self, module: &str) -> Option<&str> {
        let first = module.split('.').next()?;
        self.members.get_key_value(first).map(|(k, _)| k.as_str())
    }

    pub fn loader(&self, libs: Vec<PathBuf>) -> PackageLoader<'_> {
        PackageLoader { w: self, libs }
    }
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Finds the modules of a package and of the packages it depends on.
pub struct PackageLoader<'w> {
    w: &'w Workspace,
    libs: Vec<PathBuf>,
}

impl PackageLoader<'_> {
    fn read(sm: &mut SourceMap, root: &Path, path: &[String]) -> Option<(FileId, String)> {
        let mut p = root.to_path_buf();
        for (i, seg) in path.iter().enumerate() {
            if i + 1 == path.len() {
                p.push(format!("{seg}.cirka"));
            } else {
                p.push(seg);
            }
        }
        let text = std::fs::read_to_string(&p).ok()?;
        let id = sm.add(p.display().to_string(), text.clone());
        Some((id, text))
    }
}

impl Loader for PackageLoader<'_> {
    fn load(&mut self, sm: &mut SourceMap, path: &[String]) -> Option<(FileId, String)> {
        if let Some(m) = path.first().and_then(|f| self.w.members.get(f)) {
            return Self::read(sm, &m.package.src_dir(), path);
        }
        if let Some(found) = Self::read(sm, &self.w.root.src_dir(), path) {
            return Some(found);
        }
        self.libs.iter().find_map(|r| Self::read(sm, r, path))
    }

    fn forbidden(&self, path: &[String], importer: &str) -> Option<String> {
        let target = path.first()?;
        if !self.w.members.contains_key(target) {
            return None;
        }
        let owner = self.w.owner(importer);
        if owner == Some(target.as_str()) {
            return None;
        }
        let (name, deps) = match owner {
            Some(o) => (
                self.w.members[o].package.name.as_str(),
                &self.w.members[o].deps,
            ),
            None => (self.w.root.name.as_str(), &self.w.root_deps),
        };
        if deps.contains(target) {
            return None;
        }
        Some(format!(
            "module `{}` belongs to package `{}`, which `{name}` does not depend on",
            path.join("."),
            self.w.members[target].package.name
        ))
    }

    fn not_found_hint(&self, path: &[String]) -> Option<String> {
        let mut ms: Vec<(&String, &Member)> = self.w.members.iter().collect();
        ms.sort_by_key(|(ns, _)| *ns);
        for (ns, m) in ms {
            let mut p = m.package.src_dir();
            for seg in &path[..path.len().saturating_sub(1)] {
                p.push(seg);
            }
            p.push(format!("{}.cirka", path.last()?));
            if p.exists() {
                return Some(format!(
                    "package `{}` has this module outside its namespace: its modules must be named `{ns}` or `{ns}.…`",
                    m.package.name
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package() {
        let p = Package::parse(
            "# a package\n[package]\nname = \"hello\"\nversion = \"0.1.0\"\nedition = \"0.0.3\"\n",
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
            Package::parse("[package]\nedition = \"0.0.3\"\n", PathBuf::new())
                .unwrap_err()
                .contains("name")
        );
    }

    #[test]
    fn parses_dependencies() {
        let p = Package::parse(
            "[package]\nname = \"app\"\nedition = \"0.0.3\"\n\
             [dependencies]\nutil = { path = \"../util\" }\n",
            PathBuf::from("x"),
        )
        .unwrap();
        assert_eq!(p.dependencies.len(), 1);
        assert_eq!(p.dependencies[0].name, "util");
        assert_eq!(p.dependencies[0].path, PathBuf::from("../util"));
        for bad in [
            "util = \"1\"",
            "util = { version = \"1\" }",
            "util = { path = 1 }",
        ] {
            let text =
                format!("[package]\nname = \"a\"\nedition = \"0.0.3\"\n[dependencies]\n{bad}\n");
            assert!(Package::parse(&text, PathBuf::new()).is_err(), "{bad}");
        }
    }

    #[test]
    fn namespaces_are_pascal_case() {
        assert_eq!(namespace_of("util").as_deref(), Some("Util"));
        assert_eq!(namespace_of("math-tools").as_deref(), Some("MathTools"));
        assert_eq!(namespace_of("a_b").as_deref(), Some("AB"));
        assert_eq!(namespace_of("1x"), None);
        assert_eq!(namespace_of(""), None);
    }
}
