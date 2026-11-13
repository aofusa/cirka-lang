//! Workspace-level structural tests (FEAT-0001).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn deps_of(krate: &str) -> Vec<String> {
    let text =
        std::fs::read_to_string(root().join("crates").join(krate).join("Cargo.toml")).unwrap();
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if in_deps && let Some((name, _)) = line.split_once(['.', '=']) {
            let name = name.trim();
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Shared crates must not depend on either front end, and the two front
/// ends must not depend on each other (01-architecture.md §3).
#[test]
fn dependency_direction() {
    for entry in std::fs::read_dir(root().join("crates")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let deps = deps_of(&name);
        let is_achc = name.starts_with("achc");
        let is_cl = name.starts_with("cirkalang") || name == "cirka";
        for d in &deps {
            if !is_achc {
                assert!(!d.starts_with("achc"), "{name} must not depend on {d}");
            }
            if !is_cl {
                assert!(!d.starts_with("cirkalang"), "{name} must not depend on {d}");
            }
        }
    }
}

fn run_bin(name: &str, args: &[&str]) -> String {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["run", "-q", "-p", name, "--"])
        .args(args)
        .current_dir(root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn versions() {
    let v = run_bin("cirka", &["--version"]);
    assert!(v.starts_with("CirkaLang v0.0."), "{v}");
    let v = run_bin("achc", &["--version"]);
    assert!(v.starts_with("aCHC "), "{v}");
}

/// The CirkaLang version under development has a language specification
/// (docs/design/cirkalang/spec-v<version>.md, FEAT-0200) that names it and
/// lists what is out of scope.
#[test]
fn cirkalang_spec_for_current_version() {
    let v = env!("CARGO_PKG_VERSION");
    let v = v.split('-').next().unwrap();
    let p = root().join(format!("docs/design/cirkalang/spec-v{v}.md"));
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert!(text.contains(&format!("CirkaLang v{v}")));
    // A later version may be written as the differences from an earlier
    // specification, which it links (spec-v0.0.2.md); that one is then
    // checked for the full sections.
    let full = match text.find("からの差分") {
        Some(_) => {
            let base = text
                .split("](spec-v")
                .nth(1)
                .and_then(|r| r.split(".md)").next())
                .unwrap_or_else(|| panic!("{}: no link to the base spec", p.display()));
            let bp = root().join(format!("docs/design/cirkalang/spec-v{base}.md"));
            std::fs::read_to_string(&bp).unwrap_or_else(|e| panic!("{}: {e}", bp.display()))
        }
        None => text.clone(),
    };
    for section in [
        "## 3. 構文 (EBNF)",
        "## 4. 型システム",
        "## 6. 共通 Core への変換",
    ] {
        assert!(full.contains(section), "spec lacks `{section}`");
    }
    assert!(text.contains("対象外"), "spec lacks the out-of-scope list");
}

/// Relative links in the documents under docs/ (design, tasks) and the top
/// level point at files that exist (FEAT-0132: the evaluation report and
/// the design documents cite each other and the tickets).
#[test]
fn doc_links_resolve() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                // docs/artifacts is git-ignored scratch output.
                if !p.ends_with("artifacts") {
                    walk(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root().join("docs"), &mut files);
    files.push(root().join("AGENTS.md"));
    let mut broken = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let Some(j) = rest.find(')') else { break };
            let target = &rest[..j];
            rest = &rest[j..];
            let path = target.split('#').next().unwrap();
            if path.is_empty()
                || path.contains("://")
                || path.starts_with("mailto:")
                || path.contains(' ')
            {
                continue;
            }
            if !f.parent().unwrap().join(path).exists() {
                broken.push(format!(
                    "{}: {target}",
                    f.strip_prefix(root()).unwrap().display()
                ));
            }
        }
    }
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
}
