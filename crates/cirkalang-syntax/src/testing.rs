//! Tests in source files (12-cirkalang.md §11, FEAT-0216).
//!
//! `test "name" = e` and `tests where { decls }` are removed right after
//! parsing in normal builds ([`strip`]). Test builds ([`expand`]) turn each
//! test into a private definition `test$N : IO ()` and lift the declarations
//! of `tests` blocks to the top level.

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
    pub span: Span,
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
    decls.push(Decl {
        private: true,
        kind: DeclKind::Sig {
            names: vec![(def.clone(), name_span)],
            ty: io_unit,
            totality: None,
        },
        span: sp,
    });
    decls.push(Decl {
        private: true,
        kind: DeclKind::Clause {
            name: def.clone(),
            name_span,
            pats: Vec::new(),
            rhs: body,
            measure: None,
        },
        span: sp,
    });
    out.test_only.insert(def.clone());
    out.tests.push(TestDecl {
        name,
        def,
        span: name_span,
    });
}
