//! Tool configuration read from the environment (`.env`, see `.env.sample`).
//!
//! External tool paths and versions are never hard-coded elsewhere; every
//! component that runs `clang`, `z3`, ... goes through [`Config`].

use std::path::PathBuf;

/// Environment-dependent settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `CIRKA_CLANG`: compiles generated LLVM IR and links.
    pub clang: String,
    /// `CIRKA_OPT`: LLVM `opt` (IR verification in tests).
    pub opt: String,
    /// `CIRKA_LLC`: LLVM `llc`.
    pub llc: String,
    /// `CIRKA_TARGET`: target triple.
    pub target: String,
    /// `CIRKA_RUNTIME_DIR`: directory containing `libcirka_rt.a`
    /// (release) and `debug/libcirka_rt.a`; empty = auto-detect.
    pub runtime_dir: Option<PathBuf>,
    /// `CIRKA_Z3`: SMT solver; `None` disables verification.
    pub z3: Option<String>,
    /// `CIRKA_VERIFY_TIMEOUT`: per-query timeout in milliseconds.
    pub verify_timeout_ms: u64,
    /// `CIRKA_LOG`: log level.
    pub log: String,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            clang: "clang".into(),
            opt: "opt".into(),
            llc: "llc".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            runtime_dir: None,
            z3: None,
            verify_timeout_ms: 10_000,
            log: "info".into(),
        }
    }
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Config {
        Config::from_lookup(|k| std::env::var(k).ok())
    }

    /// Reads the configuration through `get` (testable).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Config {
        let d = Config::default();
        let s = |k: &str, dflt: String| get(k).filter(|v| !v.is_empty()).unwrap_or(dflt);
        Config {
            clang: s("CIRKA_CLANG", d.clang),
            opt: s("CIRKA_OPT", d.opt),
            llc: s("CIRKA_LLC", d.llc),
            target: s("CIRKA_TARGET", d.target),
            runtime_dir: get("CIRKA_RUNTIME_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            z3: get("CIRKA_Z3").filter(|v| !v.is_empty()),
            verify_timeout_ms: get("CIRKA_VERIFY_TIMEOUT")
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.verify_timeout_ms),
            log: s("CIRKA_LOG", d.log),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn reads_values_and_defaults() {
        let env: HashMap<&str, &str> = [
            ("CIRKA_CLANG", "clang-18"),
            ("CIRKA_Z3", ""),
            ("CIRKA_VERIFY_TIMEOUT", "500"),
            ("CIRKA_RUNTIME_DIR", "/rt"),
        ]
        .into_iter()
        .collect();
        let c = Config::from_lookup(|k| env.get(k).map(|v| v.to_string()));
        assert_eq!(c.clang, "clang-18");
        assert_eq!(c.opt, "opt");
        assert_eq!(c.z3, None);
        assert_eq!(c.verify_timeout_ms, 500);
        assert_eq!(c.runtime_dir, Some(PathBuf::from("/rt")));
        assert_eq!(c.target, "x86_64-unknown-linux-gnu");
    }

    /// Every CIRKA_* variable read by the driver is documented in .env.sample.
    #[test]
    fn env_sample_documents_variables() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let sample = std::fs::read_to_string(root.join(".env.sample")).unwrap();
        let full = include_str!("config.rs");
        let src = &full[..full.find("#[cfg(test)]").unwrap()];
        let mut i = 0;
        while let Some(p) = src[i..].find("\"CIRKA_") {
            let start = i + p + 1;
            let end = start + src[start..].find('"').unwrap();
            let var = &src[start..end];
            assert!(
                sample.contains(&format!("{var}=")),
                "{var} missing in .env.sample"
            );
            i = end;
        }
    }
}
