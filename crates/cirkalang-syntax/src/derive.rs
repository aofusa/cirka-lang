//! `deriving`: expands `data ... deriving (Eq, Ord, Show, Drop, Release)`
//! into `impl` declarations on the AST (spec-v0.0.2.md §3.5, §5.3). Names
//! of the Prelude are written qualified, so the expansion does not depend
//! on what the module has in scope.

use crate::ast::*;
use cirka_diagnostics::{Diagnostic, Span};

/// Adds the generated `impl`s after their data declarations (run after
/// `tests` blocks have been flattened). Returns the errors for derivations
/// that are not supported.
pub fn expand(m: &mut Module) -> Vec<Diagnostic> {
    let mut errs = Vec::new();
    let mut out = Vec::with_capacity(m.decls.len());
    for d in std::mem::take(&mut m.decls) {
        match &d.kind {
            DeclKind::Data(dd) if !dd.deriving.is_empty() => {
                let gen_: Vec<Decl> = derive_all(dd, &mut errs);
                out.push(d);
                out.extend(gen_);
            }
            _ => out.push(d),
        }
    }
    m.decls = out;
    errs
}

struct Field {
    runtime: bool,
}

struct Ctor {
    name: String,
    fields: Vec<Field>,
}

fn derive_all(dd: &DataDecl, errs: &mut Vec<Diagnostic>) -> Vec<Decl> {
    let cannot = |sp: Span, what: String| {
        Diagnostic::error(format!("cannot derive for `{}`: {what}", dd.name))
            .with_code("E1116")
            .with_label(sp, "this derivation")
    };
    let mut out = Vec::new();
    for (q, sp) in &dd.deriving {
        if !matches!(q.name.as_str(), "Eq" | "Ord" | "Show" | "Drop" | "Release") {
            errs.push(
                Diagnostic::error(format!("`{q}` cannot be derived"))
                    .with_code("E1116")
                    .with_label(
                        *sp,
                        "only `Eq`, `Ord`, `Show`, `Drop` and `Release` can be derived",
                    ),
            );
            continue;
        }
        // The finalizers of linear types finalize their fields.
        let fin = matches!(q.name.as_str(), "Drop" | "Release");
        let kind_ok = |t: &Expr| t.kind == ExprKind::Type || (fin && t.kind == ExprKind::LType);
        if fin && !dd.sig.as_ref().is_none_or(kind_ok) {
            errs.push(cannot(
                *sp,
                "the type has indices (write the `impl` by hand)".into(),
            ));
            continue;
        }
        if !fin && !dd.sig.as_ref().is_none_or(|t| t.kind == ExprKind::Type) {
            errs.push(cannot(
                *sp,
                "the type has indices or is linear (write the `impl` by hand)".into(),
            ));
            continue;
        }
        if !dd.params.iter().all(|p| p.ty.as_ref().is_none_or(kind_ok)) {
            errs.push(cannot(*sp, "a parameter is not of kind `Type`".into()));
            continue;
        }
        let ctors: Vec<Ctor> = dd.cons.iter().map(ctor).collect();
        let items = match q.name.as_str() {
            "Eq" => eq(&ctors, *sp),
            "Ord" => ord(&ctors, *sp),
            "Drop" => drop_(&ctors, *sp),
            "Release" => release(&ctors, *sp),
            _ => show(&ctors, *sp),
        };
        let trait_ = Expr {
            kind: ExprKind::Var(q.clone()),
            span: *sp,
        };
        let params: Vec<String> = dd
            .params
            .iter()
            .flat_map(|p| p.names.iter().map(|n| n.0.clone()))
            .collect();
        let tvar = |n: &str| Expr {
            kind: ExprKind::Var(QName::simple(n)),
            span: *sp,
        };
        let self_ty = params.iter().fold(
            Expr {
                kind: ExprKind::Var(QName::simple(dd.name.clone())),
                span: *sp,
            },
            |f, p| app(f, tvar(p), *sp),
        );
        let context = params
            .iter()
            .map(|p| app(trait_.clone(), tvar(p), *sp))
            .collect();
        out.push(Decl {
            private: false,
            span: *sp,
            kind: DeclKind::Impl(ImplDecl {
                binders: Vec::new(),
                context,
                head: app(trait_, self_ty, *sp),
                items,
            }),
        });
    }
    out
}

/// The explicit fields of a constructor.
fn ctor(c: &ConDecl) -> Ctor {
    let mut fields = Vec::new();
    match &c.body {
        ConBody::Fields(fs) => fields.extend(fs.iter().map(|_| Field { runtime: true })),
        ConBody::Sig(t) => {
            let mut t = t;
            while let ExprKind::Pi {
                mult,
                implicit,
                names,
                cod,
                ..
            } = &t.kind
            {
                if !implicit {
                    for _ in 0..names.len().max(1) {
                        fields.push(Field {
                            runtime: *mult != Some(Mult::Zero),
                        });
                    }
                }
                t = cod;
            }
        }
    }
    Ctor {
        name: c.name.clone(),
        fields,
    }
}

fn app(f: Expr, a: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::App(Box::new(f), Box::new(a)),
        span,
    }
}

fn prelude(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName {
            module: vec!["Prelude".to_string()],
            name: n.to_string(),
        }),
        span,
    }
}

fn app2(f: &str, a: Expr, b: Expr, span: Span) -> Expr {
    app(app(prelude(f, span), a, span), b, span)
}

fn local(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName::simple(n)),
        span,
    }
}

fn int(n: u128, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Lit(Lit::Int(n)),
        span,
    }
}

fn str_(s: String, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Lit(Lit::Str(s)),
        span,
    }
}

/// `(C p1 p2 ...)`; only the runtime fields are bound.
fn con_pat(c: &Ctor, base: &str, span: Span) -> Pat {
    let args = c
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| Pat {
            span,
            kind: if f.runtime {
                PatKind::Var(format!("{base}{i}"))
            } else {
                PatKind::Wild
            },
        })
        .collect();
    Pat {
        span,
        kind: PatKind::Con(QName::simple(c.name.clone()), args),
    }
}

fn clause(name: &str, pats: Vec<Pat>, rhs: Expr, span: Span) -> Decl {
    Decl {
        private: false,
        span,
        kind: DeclKind::Clause {
            name: name.to_string(),
            name_span: span,
            pats,
            rhs,
        },
    }
}

fn wild(span: Span) -> Pat {
    Pat {
        kind: PatKind::Wild,
        span,
    }
}

fn runtime(c: &Ctor) -> Vec<usize> {
    (0..c.fields.len())
        .filter(|i| c.fields[*i].runtime)
        .collect()
}

fn eq(ctors: &[Ctor], sp: Span) -> Vec<Decl> {
    let mut out = Vec::new();
    for c in ctors {
        let fs = runtime(c);
        let cmp = |i: usize| {
            app2(
                "==",
                local(&format!("a{i}"), sp),
                local(&format!("b{i}"), sp),
                sp,
            )
        };
        // Evaluation is strict: `if` stops at the first difference.
        let mut rhs = match fs.last() {
            Some(i) => cmp(*i),
            None => prelude("True", sp),
        };
        for i in fs.iter().rev().skip(1) {
            rhs = Expr {
                span: sp,
                kind: ExprKind::If(
                    Box::new(cmp(*i)),
                    Box::new(rhs),
                    Box::new(prelude("False", sp)),
                ),
            };
        }
        out.push(clause(
            "==",
            vec![con_pat(c, "a", sp), con_pat(c, "b", sp)],
            rhs,
            sp,
        ));
    }
    if ctors.len() > 1 {
        out.push(clause(
            "==",
            vec![wild(sp), wild(sp)],
            prelude("False", sp),
            sp,
        ));
    }
    out
}

/// `drop (C a0 a1) = case drop a0 of MkUnit => drop a1`: the fields are
/// dropped in order.
fn drop_(ctors: &[Ctor], sp: Span) -> Vec<Decl> {
    let mut out = Vec::new();
    for c in ctors {
        let fs = runtime(c);
        let call = |i: usize| app(prelude("drop", sp), local(&format!("a{i}"), sp), sp);
        let mut rhs = match fs.last() {
            Some(i) => call(*i),
            None => prelude("MkUnit", sp),
        };
        for i in fs.iter().rev().skip(1) {
            rhs = Expr {
                span: sp,
                kind: ExprKind::Case(
                    Box::new(call(*i)),
                    vec![Alt {
                        pat: Pat {
                            span: sp,
                            kind: PatKind::Con(
                                QName {
                                    module: vec!["Prelude".to_string()],
                                    name: "MkUnit".to_string(),
                                },
                                Vec::new(),
                            ),
                        },
                        rhs,
                    }],
                ),
            };
        }
        out.push(clause("drop", vec![con_pat(c, "a", sp)], rhs, sp));
    }
    out
}

/// `release (C a0 a1) = primBind (release a0) (\_ => release a1)`: the
/// fields are released in order.
fn release(ctors: &[Ctor], sp: Span) -> Vec<Decl> {
    let mut out = Vec::new();
    for c in ctors {
        let fs = runtime(c);
        let call = |i: usize| app(prelude("release", sp), local(&format!("a{i}"), sp), sp);
        let mut rhs = match fs.last() {
            Some(i) => call(*i),
            None => app(prelude("primPure", sp), prelude("MkUnit", sp), sp),
        };
        for i in fs.iter().rev().skip(1) {
            let k = Expr {
                span: sp,
                kind: ExprKind::Lam(
                    vec![LamBinder {
                        name: None,
                        span: sp,
                        mult: None,
                        ty: None,
                    }],
                    Box::new(rhs),
                ),
            };
            rhs = app2("primBind", call(*i), k, sp);
        }
        out.push(clause("release", vec![con_pat(c, "a", sp)], rhs, sp));
    }
    out
}

/// `case x of { C1 _ _ => 0 ; C2 => 1 }`.
fn tag(ctors: &[Ctor], x: &str, sp: Span) -> Expr {
    let alts = ctors
        .iter()
        .enumerate()
        .map(|(i, c)| Alt {
            pat: Pat {
                span: sp,
                kind: PatKind::Con(
                    QName::simple(c.name.clone()),
                    c.fields.iter().map(|_| wild(sp)).collect(),
                ),
            },
            rhs: Expr {
                span: sp,
                kind: ExprKind::Ann(Box::new(int(i as u128, sp)), Box::new(prelude("Int", sp))),
            },
        })
        .collect();
    Expr {
        span: sp,
        kind: ExprKind::Case(Box::new(local(x, sp)), alts),
    }
}

fn ord(ctors: &[Ctor], sp: Span) -> Vec<Decl> {
    let mut out = Vec::new();
    for c in ctors {
        let fs = runtime(c);
        let cmp = |i: usize| {
            app2(
                "compare",
                local(&format!("a{i}"), sp),
                local(&format!("b{i}"), sp),
                sp,
            )
        };
        let mut rhs = match fs.last() {
            Some(i) => cmp(*i),
            None => prelude("EQ", sp),
        };
        for i in fs.iter().rev().skip(1) {
            rhs = Expr {
                span: sp,
                kind: ExprKind::Case(
                    Box::new(cmp(*i)),
                    vec![
                        Alt {
                            pat: Pat {
                                span: sp,
                                kind: PatKind::Con(
                                    QName {
                                        module: vec!["Prelude".to_string()],
                                        name: "EQ".to_string(),
                                    },
                                    Vec::new(),
                                ),
                            },
                            rhs,
                        },
                        Alt {
                            pat: Pat {
                                span: sp,
                                kind: PatKind::Var("r".to_string()),
                            },
                            rhs: local("r", sp),
                        },
                    ],
                ),
            };
        }
        out.push(clause(
            "compare",
            vec![con_pat(c, "a", sp), con_pat(c, "b", sp)],
            rhs,
            sp,
        ));
    }
    if ctors.len() > 1 {
        let var = |n: &str| Pat {
            kind: PatKind::Var(n.to_string()),
            span: sp,
        };
        out.push(clause(
            "compare",
            vec![var("x"), var("y")],
            app2("compare", tag(ctors, "x", sp), tag(ctors, "y", sp), sp),
            sp,
        ));
    }
    out
}

fn show(ctors: &[Ctor], sp: Span) -> Vec<Decl> {
    let mut out = Vec::new();
    for c in ctors {
        let fs = runtime(c);
        let shown = if c.name.starts_with(':') {
            format!("({})", c.name)
        } else {
            c.name.clone()
        };
        let d = |n: &str| Pat {
            kind: PatKind::Var(n.to_string()),
            span: sp,
        };
        if fs.is_empty() {
            out.push(clause(
                "showPrec",
                vec![wild(sp), con_pat(c, "a", sp)],
                str_(shown, sp),
                sp,
            ));
            continue;
        }
        let mut rhs = str_(shown, sp);
        for i in fs {
            rhs = app2("++", rhs, str_(" ".to_string(), sp), sp);
            rhs = app2(
                "++",
                rhs,
                app2("showPrec", int(11, sp), local(&format!("a{i}"), sp), sp),
                sp,
            );
        }
        let paren = app2(">", local("d", sp), int(10, sp), sp);
        out.push(clause(
            "showPrec",
            vec![d("d"), con_pat(c, "a", sp)],
            app2("showParen", paren, rhs, sp),
            sp,
        ));
    }
    out
}
