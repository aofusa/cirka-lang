//! The parts of `extern "C"` blocks (CirkaExtern, FEAT-0130, 08-ffi.md §7.4)
//! that need resolved names: the scope of borrowed pointers, the linear
//! `OwnedPtr` parameters, and the `Release` instances of `free with`.

use crate::err;
use achc_syntax::ast::*;
use cirka_diagnostics::{Diagnostic, Span};
use std::collections::HashSet;

/// The module defining the types of extern blocks (re-exported by
/// `Cirka.Extern`).
const EXTERN: &str = "Unsafe.Extern";

fn qname(module: &str, name: &str, span: Span) -> QName {
    QName {
        qual: Some(module.into()),
        name: name.into(),
        span,
    }
}

fn mk(kind: TypeKind, span: Span) -> Type {
    Type { kind, span }
}

fn is_con(t: &Type, module: &str, name: &str) -> bool {
    matches!(&t.kind, TypeKind::Con(q) if q.qual.as_deref() == Some(module) && q.name == name)
}

/// `head` applied to one argument: the argument.
fn applied<'a>(t: &'a Type, module: &str, name: &str) -> Option<&'a Type> {
    match &t.kind {
        TypeKind::App(f, a) if is_con(f, module, name) => Some(a),
        _ => None,
    }
}

/// `T` of `OwnedPtr T`, also under `Nullable` / `Maybe`.
fn owned_target(t: &Type) -> Option<&Type> {
    let inner = applied(t, EXTERN, "Nullable")
        .or_else(|| applied(t, "Cirka.Base", "Maybe"))
        .unwrap_or(t);
    applied(inner, EXTERN, "OwnedPtr")
}

/// The parameters (with their multiplicity) and the result of a type.
fn split(t: &Type) -> (Vec<(Option<&Multiplicity>, &Type)>, &Type) {
    let mut ps = Vec::new();
    let mut t = t;
    loop {
        match &t.kind {
            TypeKind::Forall(_, b) | TypeKind::Qual(_, b) => t = b,
            TypeKind::Fun(a, b) => {
                ps.push((None, &**a));
                t = b;
            }
            TypeKind::MulFun(m, a, b) => {
                ps.push((Some(m), &**a));
                t = b;
            }
            _ => return (ps, t),
        }
    }
}

/// The value of a result in `IO` or a borrow monad.
fn run_result(t: &Type) -> &Type {
    match &t.kind {
        TypeKind::App(f, a) => match &f.kind {
            _ if is_con(f, crate::PRIM, "IO") => a,
            TypeKind::App(g, _)
                if is_con(g, EXTERN, "BorrowIO") || is_con(g, "Unsafe.Borrow", "Borrow") =>
            {
                a
            }
            _ => t,
        },
        _ => t,
    }
}

fn tyvars(t: &Type, out: &mut HashSet<String>) {
    match &t.kind {
        TypeKind::Var(v) => {
            out.insert(v.clone());
        }
        TypeKind::Con(_) => {}
        TypeKind::App(a, b) | TypeKind::Fun(a, b) => {
            tyvars(a, out);
            tyvars(b, out);
        }
        TypeKind::MulFun(m, a, b) => {
            if let Multiplicity::Var(v) = m {
                out.insert(v.clone());
            }
            tyvars(a, out);
            tyvars(b, out);
        }
        TypeKind::List(a) | TypeKind::Kinded(a, _) => tyvars(a, out),
        TypeKind::Tuple(ts) => ts.iter().for_each(|t| tyvars(t, out)),
        TypeKind::Qual(cs, b) => {
            for c in cs {
                c.types.iter().for_each(|t| tyvars(t, out));
            }
            tyvars(b, out);
        }
        TypeKind::Forall(vs, b) => {
            vs.iter().for_each(|(v, _)| {
                out.insert(v.name.clone());
            });
            tyvars(b, out);
        }
    }
}

/// `BorrowedPtr T` becomes `BorrowedPtr r T`; a `BorrowedPtr` written with
/// its scope is an error.
fn add_scope(t: &Type, r: &str, found: &mut bool, errors: &mut Vec<Diagnostic>) -> Type {
    let go = |t: &Type, found: &mut bool, errors: &mut Vec<Diagnostic>| {
        Box::new(add_scope(t, r, found, errors))
    };
    let kind = match &t.kind {
        TypeKind::App(f, a) => {
            if is_con(f, EXTERN, "BorrowedPtr") {
                *found = true;
                let scoped = mk(
                    TypeKind::App(f.clone(), Box::new(mk(TypeKind::Var(r.into()), f.span))),
                    f.span,
                );
                TypeKind::App(Box::new(scoped), go(a, found, errors))
            } else if matches!(&f.kind, TypeKind::App(g, _) if is_con(g, EXTERN, "BorrowedPtr")) {
                errors.push(
                    err(
                        "E0619",
                        t.span,
                        "the scope of a borrowed pointer is added by the compiler",
                    )
                    .with_note("write `BorrowedPtr T` in an extern block"),
                );
                return t.clone();
            } else {
                TypeKind::App(go(f, found, errors), go(a, found, errors))
            }
        }
        TypeKind::Fun(a, b) => TypeKind::Fun(go(a, found, errors), go(b, found, errors)),
        TypeKind::MulFun(m, a, b) => {
            TypeKind::MulFun(m.clone(), go(a, found, errors), go(b, found, errors))
        }
        TypeKind::List(a) => TypeKind::List(go(a, found, errors)),
        TypeKind::Kinded(a, k) => TypeKind::Kinded(go(a, found, errors), k.clone()),
        TypeKind::Tuple(ts) => {
            TypeKind::Tuple(ts.iter().map(|t| add_scope(t, r, found, errors)).collect())
        }
        TypeKind::Qual(cs, b) => TypeKind::Qual(cs.clone(), go(b, found, errors)),
        TypeKind::Forall(vs, b) => TypeKind::Forall(vs.clone(), go(b, found, errors)),
        TypeKind::Var(_) | TypeKind::Con(_) => return t.clone(),
    };
    mk(kind, t.span)
}

/// The result `IO t` becomes `BorrowIO r t`, a pure result `t` becomes
/// `Borrow r t`; a quantified type also binds `r`.
fn lift_result(t: Type, r: &str) -> Type {
    let span = t.span;
    let kind = match t.kind {
        TypeKind::Forall(mut vs, b) => {
            vs.push((
                Ident {
                    name: r.into(),
                    span,
                },
                None,
            ));
            TypeKind::Forall(vs, Box::new(lift_result(*b, r)))
        }
        TypeKind::Qual(cs, b) => TypeKind::Qual(cs, Box::new(lift_result(*b, r))),
        TypeKind::Fun(a, b) => TypeKind::Fun(a, Box::new(lift_result(*b, r))),
        TypeKind::MulFun(m, a, b) => TypeKind::MulFun(m, a, Box::new(lift_result(*b, r))),
        kind => {
            let res = mk(kind, span);
            let (monad, value) = match &res.kind {
                TypeKind::App(f, a) if is_con(f, crate::PRIM, "IO") => {
                    (qname(EXTERN, "BorrowIO", span), (**a).clone())
                }
                _ => (qname("Unsafe.Borrow", "Borrow", span), res.clone()),
            };
            let head = mk(
                TypeKind::App(
                    Box::new(mk(TypeKind::Con(monad), span)),
                    Box::new(mk(TypeKind::Var(r.into()), span)),
                ),
                span,
            );
            TypeKind::App(Box::new(head), Box::new(value))
        }
    };
    mk(kind, span)
}

/// The type of a function of an extern block, after name resolution:
/// borrowed pointers get the scope `r` and the result is lifted into the
/// borrow monad. The type checker checks the rest on the type with its
/// synonyms expanded (`OwnedPtr` parameters and results, scopes).
pub(crate) fn extern_sig(t: &Type, errors: &mut Vec<Diagnostic>) -> Type {
    let mut used = HashSet::new();
    tyvars(t, &mut used);
    let r = std::iter::once("r".to_string())
        .chain((1..).map(|i| format!("r{i}")))
        .find(|v| !used.contains(v))
        .unwrap();
    let mut found = false;
    let t = add_scope(t, &r, &mut found, errors);
    if found { lift_result(t, &r) } else { t }
}

/// The `Release (OwnedPtr T)` instances of the `free with` attributes of
/// the module's extern blocks (after renaming).
pub(crate) fn extern_instances(
    module: &str,
    decls: &[Decl],
    errors: &mut Vec<Diagnostic>,
) -> Vec<Decl> {
    let funs: Vec<(&ForeignDecl, &ExternAttrs)> = decls
        .iter()
        .filter_map(|d| match d {
            Decl::Foreign(f) => f.ext.as_ref().map(|x| (f, x)),
            _ => None,
        })
        .collect();
    // (T, h, the first declaration naming them)
    let mut frees: Vec<(String, String, Span)> = Vec::new();
    for (f, x) in &funs {
        let Some(h) = &x.free_with else {
            continue;
        };
        let (_, res) = split(&f.ty);
        let target = owned_target(run_result(res)).and_then(|t| match &t.kind {
            TypeKind::Con(q) if q.qual.as_deref() == Some(module) => Some(q.name.clone()),
            _ => None,
        });
        let Some(target) = target.filter(|t| x.opaque.contains(t)) else {
            errors.push(
                err(
                    "E0619",
                    f.span,
                    format!(
                        "`free with {}` needs a result `OwnedPtr T` or `Nullable (OwnedPtr T)` \
                         for an opaque type `T` of the block",
                        h.name
                    ),
                )
                .with_note("declare the type in the block with `type T`"),
            );
            continue;
        };
        let freer = funs
            .iter()
            .find(|(g, y)| y.block == x.block && g.name.name == h.name);
        let shape_ok = freer.is_some_and(|(g, _)| {
            let (ps, _) = split(&g.ty);
            matches!(ps.as_slice(), [(Some(Multiplicity::One), p)]
                if applied(p, EXTERN, "OwnedPtr").is_some_and(|t| matches!(&t.kind,
                    TypeKind::Con(q) if q.qual.as_deref() == Some(module) && q.name == target)))
        });
        if !shape_ok {
            let msg = match freer {
                None => format!("`{}` is not a function of this extern block", h.name),
                Some(_) => format!(
                    "`{}` cannot free `OwnedPtr {target}`: its type must be `OwnedPtr {target} %1 -> ...`",
                    h.name
                ),
            };
            errors.push(err("E0619", h.span, msg));
            continue;
        }
        match frees.iter().find(|(t, _, _)| *t == target) {
            Some((_, h0, _)) if *h0 != h.name => errors.push(err(
                "E0619",
                h.span,
                format!(
                    "`OwnedPtr {target}` is freed with both `{h0}` and `{}`",
                    h.name
                ),
            )),
            Some(_) => {}
            None => frees.push((target, h.name.clone(), f.span)),
        }
    }
    frees
        .into_iter()
        .map(|(target, h, span)| {
            let ty = mk(
                TypeKind::App(
                    Box::new(mk(TypeKind::Con(qname(EXTERN, "OwnedPtr", span)), span)),
                    Box::new(mk(TypeKind::Con(qname(module, &target, span)), span)),
                ),
                span,
            );
            let var = |m: &str, n: &str| Expr {
                kind: ExprKind::Var(qname(m, n, span)),
                span,
            };
            let body = Expr {
                kind: ExprKind::App(
                    Box::new(var(EXTERN, "releaseWith")),
                    Box::new(var(module, &h)),
                ),
                span,
            };
            Decl::Instance(InstDecl {
                context: Vec::new(),
                class: qname("Cirka.Ownership", "Release", span),
                types: vec![ty],
                decls: vec![Decl::Fun(
                    Ident {
                        name: "Cirka.Ownership.release".into(),
                        span,
                    },
                    Vec::new(),
                    Rhs {
                        body: RhsBody::Plain(body),
                        wheres: Vec::new(),
                    },
                    span,
                )],
                span,
            })
        })
        .collect()
}
