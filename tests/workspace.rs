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
    for section in [
        "## 3. 構文 (EBNF)",
        "## 4. 型システム",
        "## 6. 共通 Core への変換",
        "対象外",
    ] {
        assert!(text.contains(section), "spec lacks `{section}`");
    }
}
