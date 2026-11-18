//! `effect` declarations (spec-v0.0.7.md §2.2): expanded on the AST, like
//! `deriving`, into the type of the effect's handlers and its operations.
//! The code names `Std.Effect` and the Prelude qualified; the resolver
//! makes `Std.Effect` visible qualified in a module with such declarations.
//!
//! ```text
//! effect E ps where
//!   op : A1 -> .. -> An -> R
//! ```
//!
//! becomes
//!
//! ```text
//! data E ps = MkE (A1 -> .. -> An -> IO R) ..
//! op : Std.Effect.Has (E ps) es => A1 -> .. -> An -> Std.Effect.Eff es R
//! op x1 .. xn = Std.Effect.perform (\h => case h of MkE f1 .. fk => fi x1 .. xn)
//! ```
//!
//! A handler of `E` is a value of `E ps`: the implementations of the
//! operations, in the order of the declaration.

use crate::ast::*;
use crate::cextern::{app, apps, arrow, clause, pcon, prelude, pvar, sig, var};
use cirka_diagnostics::Span;

/// The module of the effect library.
pub const MODULE: &str = "Std.Effect";

/// The constructor of the handlers of effect `e`.
pub fn con_name(e: &str) -> String {
    format!("Mk{e}")
}

/// Expands the effect declarations of `m`, also those of `tests` blocks.
/// Returns whether there were any (the module then uses `Std.Effect`).
pub fn expand(m: &mut Module) -> bool {
    let mut any = false;
    m.decls = expand_decls(std::mem::take(&mut m.decls), &mut any);
    any
}

fn expand_decls(ds: Vec<Decl>, any: &mut bool) -> Vec<Decl> {
    let mut out = Vec::with_capacity(ds.len());
    for d in ds {
        match d.kind {
            DeclKind::Effect(e) => {
                *any = true;
                out.extend(effect(&e, d.private, d.origin, d.span));
            }
            DeclKind::Tests(inner) => out.push(Decl {
                kind: DeclKind::Tests(expand_decls(inner, any)),
                ..d
            }),
            kind => out.push(Decl { kind, ..d }),
        }
    }
    out
}

fn effect_name(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName {
            module: MODULE.split('.').map(String::from).collect(),
            name: n.into(),
        }),
        span,
    }
}

/// The arguments and the result of the type of an operation (the parser
/// has checked that its arrows are plain).
fn split_arrows(t: &Expr) -> (Vec<Expr>, Expr) {
    let mut args = Vec::new();
    let mut t = t;
    while let ExprKind::Pi { dom, cod, .. } = &t.kind {
        args.push((**dom).clone());
        t = cod;
    }
    (args, t.clone())
}

fn effect(e: &EffectDecl, private: bool, origin: Origin, span: Span) -> Vec<Decl> {
    let sp = e.name_span;
    let decl = |kind| Decl {
        private,
        origin,
        kind,
        span,
    };
    let con = con_name(&e.name);
    let pnames: Vec<(String, Span)> = e.params.iter().flat_map(|p| p.names.clone()).collect();
    let ety = apps(
        var(&e.name, sp),
        pnames.iter().map(|(n, s)| var(n, *s)).collect(),
        sp,
    );
    // The row variable: a name that is not a parameter.
    let mut row = "es".to_string();
    while pnames.iter().any(|(n, _)| *n == row) {
        row.push('_');
    }
    let ops: Vec<(String, Span, Vec<Expr>, Expr)> = e
        .ops
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Sig { names, ty, .. } => Some((names, ty)),
            _ => None,
        })
        .flat_map(|(names, ty)| {
            let (args, res) = split_arrows(ty);
            names
                .iter()
                .map(move |(n, s)| (n.clone(), *s, args.clone(), res.clone()))
        })
        .collect();
    let fields: Vec<Expr> = ops
        .iter()
        .map(|(_, s, args, res)| {
            let io = app(prelude("IO", *s), res.clone(), *s);
            args.iter()
                .rev()
                .fold(io, |acc, a| arrow(a.clone(), acc, *s))
        })
        .collect();
    let mut out = vec![decl(DeclKind::Data(DataDecl {
        name: e.name.clone(),
        name_span: sp,
        params: e.params.clone(),
        sig: None,
        cons: vec![ConDecl {
            name: con.clone(),
            span: sp,
            body: ConBody::Fields(fields),
            private: false,
        }],
        short: true,
        deriving: Vec::new(),
    }))];
    // The type of the handler in the operations: the parameters are not
    // in scope in their bodies.
    let hty = apps(
        var(&e.name, sp),
        pnames
            .iter()
            .map(|(_, s)| Expr {
                kind: ExprKind::Wildcard,
                span: *s,
            })
            .collect(),
        sp,
    );
    let k = ops.len();
    for (i, (name, s, args, res)) in ops.iter().enumerate() {
        let s = *s;
        let rowv = var(&row, s);
        let has = apps(effect_name("Has", s), vec![ety.clone(), rowv.clone()], s);
        let eff = apps(effect_name("Eff", s), vec![rowv, res.clone()], s);
        let body = args
            .iter()
            .rev()
            .fold(eff, |acc, a| arrow(a.clone(), acc, s));
        let ty = Expr {
            kind: ExprKind::Pi {
                mult: Some(Mult::Many),
                implicit: true,
                names: Vec::new(),
                dom: Box::new(has),
                cod: Box::new(body),
            },
            span: s,
        };
        out.push(decl(sig(name, s, ty)));
        let xs: Vec<String> = (1..=args.len()).map(|j| format!("x{j}")).collect();
        let fs: Vec<String> = (1..=k).map(|j| format!("f{j}")).collect();
        let call = apps(var(&fs[i], s), xs.iter().map(|x| var(x, s)).collect(), s);
        let select = Expr {
            kind: ExprKind::Lam(
                vec![LamBinder {
                    name: Some("h".into()),
                    span: s,
                    mult: None,
                    ty: Some(hty.clone()),
                }],
                Box::new(Expr {
                    kind: ExprKind::Case(
                        Box::new(var("h", s)),
                        vec![Alt {
                            pat: pcon(&con, fs.iter().map(|f| pvar(f, s)).collect(), s),
                            rhs: call,
                        }],
                    ),
                    span: s,
                }),
            ),
            span: s,
        };
        let rhs = app(effect_name("perform", s), select, s);
        out.push(decl(clause(
            name,
            s,
            xs.iter().map(|x| pvar(x, s)).collect(),
            rhs,
        )));
    }
    out
}
