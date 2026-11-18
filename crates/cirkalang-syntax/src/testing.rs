//! Tests in source files (12-cirkalang.md §11, FEAT-0216).
//!
//! `test "name" = e` and `tests where { decls }` are removed right after
//! parsing in normal builds ([`strip`]). Test builds ([`expand`]) turn each
//! test into a private definition `test$N : IO ()` and lift the declarations
//! of `tests` blocks to the top level. A test with capability parameters
//! (spec-v0.0.7.md §1.2) becomes `test$N : Cap K1 -> ... -> IO ()`.

use crate::ast::*;
use cirka_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

/// A test of a module after [`expand`].
#[derive(Clone, Debug, PartialEq)]
pub struct TestDecl {
    /// The name written after `test`.
    pub name: String,
    /// The generated definition (`test$N`).
    pub def: String,
    /// The capabilities it receives, in order (`"FileSystem"`, ...).
    pub caps: Vec<String>,
    pub span: Span,
}

/// The constructors of the Prelude's `Capability` (spec-v0.0.7.md §1.1).
pub const CAPABILITIES: [&str; 6] = [
    "FileSystem",
    "Network",
    "Clock",
    "Random",
    "Process",
    "Environment",
];

/// `K` of the type `Cap K`.
fn capability(ty: &Expr) -> Option<String> {
    let ExprKind::App(f, k) = &ty.kind else {
        return None;
    };
    match (&f.kind, &k.kind) {
        (ExprKind::Var(c), ExprKind::Var(k))
            if c.name == "Cap" && CAPABILITIES.contains(&k.name.as_str()) =>
        {
            Some(k.name.clone())
        }
        _ => None,
    }
}

/// The result of [`expand`].
#[derive(Debug, Default)]
pub struct Expanded {
    pub tests: Vec<TestDecl>,
    /// Names defined only for tests (by `test` and `tests` blocks).
    pub test_only: HashSet<String>,
    pub errors: Vec<Diagnostic>,
}

/// Removes the tests of `m` (normal builds: tests cost nothing).
pub fn strip(m: &mut Module) {
    m.decls
        .retain(|d| !matches!(d.kind, DeclKind::Test { .. } | DeclKind::Tests(_)));
}

fn var(name: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName::simple(name)),
        span,
    }
}

/// Turns the tests of `m` into ordinary private definitions.
pub fn expand(m: &mut Module) -> Expanded {
    let mut out = Expanded::default();
    let mut seen: HashMap<String, Span> = HashMap::new();
    let mut decls = Vec::new();
    for d in std::mem::take(&mut m.decls) {
        match d.kind {
            DeclKind::Tests(ds) => {
                for d in ds {
                    match &d.kind {
                        DeclKind::Sig { names, .. } => {
                            out.test_only.extend(names.iter().map(|n| n.0.clone()))
                        }
                        DeclKind::Clause { name, .. } => {
                            out.test_only.insert(name.clone());
                        }
                        DeclKind::Data(dd) => {
                            out.test_only.insert(dd.name.clone());
                            out.test_only.extend(dd.cons.iter().map(|c| c.name.clone()));
                        }
                        DeclKind::Trait(t) => {
                            out.test_only.insert(t.name.clone());
                            for it in &t.items {
                                if let DeclKind::Sig { names, .. } = &it.kind {
                                    out.test_only.extend(names.iter().map(|n| n.0.clone()));
                                }
                            }
                        }
                        _ => {}
                    }
                    test(&mut out, &mut seen, &mut decls, d);
                }
            }
            _ => test(&mut out, &mut seen, &mut decls, d),
        }
    }
    m.decls = decls;
    out
}

fn test(out: &mut Expanded, seen: &mut HashMap<String, Span>, decls: &mut Vec<Decl>, d: Decl) {
    let DeclKind::Test {
        name,
        name_span,
        params,
        body,
    } = d.kind
    else {
        decls.push(d);
        return;
    };
    if let Some(prev) = seen.get(&name) {
        out.errors.push(
            Diagnostic::error(format!("duplicate test `{name}`"))
                .with_code("E1112")
                .with_label(name_span, "a test with this name is already defined")
                .with_secondary(*prev, "first defined here"),
        );
        return;
    }
    seen.insert(name.clone(), name_span);
    let def = format!("test${}", out.tests.len());
    let sp = d.span;
    let io_unit = Expr {
        kind: ExprKind::App(
            Box::new(var("IO", sp)),
            Box::new(Expr {
                kind: ExprKind::Unit,
                span: sp,
            }),
        ),
        span: sp,
    };
    // The capabilities: `test "t" (fs : Cap FileSystem) = e` is
    // `test$N : (fs : Cap FileSystem) -> IO ()` with `test$N fs = e`.
    let mut caps = Vec::new();
    let mut pats = Vec::new();
    let mut ty = io_unit;
    let mut bad = false;
    for b in params.iter().rev() {
        let Some(k) = b.ty.as_ref().and_then(capability) else {
            out.errors.push(
                Diagnostic::error("a test can only receive capabilities")
                    .with_code("E1117")
                    .with_label(b.span, "not a capability")
                    .with_help(
                        "write `(fs : Cap FileSystem)`: the test runner gives a test only the \
                         capabilities it asks for (spec-v0.0.7.md §1.2)",
                    ),
            );
            bad = true;
            continue;
        };
        caps.push(k);
        pats.push(Pat {
            kind: match &b.name {
                Some(x) => PatKind::Var(x.clone()),
                None => PatKind::Wild,
            },
            span: b.span,
        });
        ty = Expr {
            kind: ExprKind::Pi {
                mult: b.mult,
                implicit: false,
                names: b.name.iter().map(|x| (x.clone(), b.span)).collect(),
                dom: Box::new(b.ty.clone().expect("a capability has a type")),
                cod: Box::new(ty),
            },
            span: sp,
        };
    }
    if bad {
        return;
    }
    caps.reverse();
    pats.reverse();
    decls.push(Decl {
        private: true,
        origin: Origin::Source,
        kind: DeclKind::Sig {
            names: vec![(def.clone(), name_span)],
            ty,
            totality: None,
            trusted: None,
        },
        span: sp,
    });
    decls.push(Decl {
        private: true,
        origin: Origin::Source,
        kind: DeclKind::Clause {
            name: def.clone(),
            name_span,
            pats,
            rhs: body,
            measure: None,
        },
        span: sp,
    });
    out.test_only.insert(def.clone());
    out.tests.push(TestDecl {
        name,
        def,
        caps,
        span: name_span,
    });
}
