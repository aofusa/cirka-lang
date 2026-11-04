//! Unpacking of strict boxed fields (like GHC's `-funbox-strict-fields`).
//!
//! A strict constructor field whose type is a box (`Int` = `I# Int#`,
//! `Float` = `F# Double#`, …) is stored unboxed: construction unwraps the
//! argument and every pattern match re-wraps the field with a lazy
//! `let x = I# u`, which the simplifier removes when `x` is only
//! scrutinised. This removes one allocation per such field.

use std::collections::{HashMap, HashSet};

use crate::Mult;
use crate::expr::*;
use crate::ty::Type;
use crate::ww::box_of;

/// A field stored unboxed: box constructor, unboxed type, boxed type.
type Field = Option<(String, Type, Type)>;

/// Per constructor: its data type and its fields.
type Plan = HashMap<String, (String, Vec<Field>)>;

/// Rewrites the data declarations and every construction and match.
/// Returns the number of fields unpacked.
pub fn unpack_strict_fields(p: &mut Program, next: &mut u32) -> usize {
    let mut plan: Plan = HashMap::new();
    for d in &p.datas {
        if d.newtype || d.linear {
            continue;
        }
        for k in &d.ctors {
            let fs: Vec<Field> = k
                .fields
                .iter()
                .zip(&k.strict)
                .map(|(t, &s)| {
                    if s {
                        box_of(p, t).map(|(c, u)| (c, u, t.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            if fs.iter().any(Option::is_some) {
                plan.insert(k.name.clone(), (d.name.clone(), fs));
            }
        }
    }
    // A recursive value binding must stay a constructor application.
    let mut rec = HashSet::new();
    for b in &p.binds {
        rec_cons(&b.expr, &mut rec);
    }
    plan.retain(|k, _| !rec.contains(k));
    if plan.is_empty() {
        return 0;
    }
    let mut n = 0;
    for d in &mut p.datas {
        for k in &mut d.ctors {
            if let Some((_, fs)) = plan.get(&k.name) {
                for (t, f) in k.fields.iter_mut().zip(fs) {
                    if let Some((_, u, _)) = f {
                        *t = u.clone();
                        n += 1;
                    }
                }
            }
        }
    }
    for b in &mut p.binds {
        let e = std::mem::replace(&mut b.expr, Expr::Lit(Lit::Unit));
        b.expr = rewrite(e, &plan, next);
    }
    n
}

/// Constructors applied directly as the right-hand side of a `let rec`.
fn rec_cons(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Let(Bind::Rec(bs), body) => {
            for (_, r) in bs {
                let mut r = r;
                while let Expr::TyLam(_, _, x) = r {
                    r = x;
                }
                if let Expr::Con(k, _, _) = r {
                    out.insert(k.clone());
                }
                rec_cons(r, out);
            }
            rec_cons(body, out);
        }
        _ => children(e, |c| rec_cons(c, out)),
    }
}

fn children(e: &Expr, mut f: impl FnMut(&Expr)) {
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
        Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => a.iter().for_each(f),
        Expr::App(x, y) | Expr::LetStrict(_, x, y) => {
            f(x);
            f(y);
        }
        Expr::TyApp(x, _) | Expr::Lam(_, x) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) => f(x),
        Expr::Let(b, body) => {
            match b {
                Bind::NonRec(_, r) => f(r),
                Bind::Rec(bs) => bs.iter().for_each(|(_, r)| f(r)),
            }
            f(body);
        }
        Expr::Case(s, _, _, alts) => {
            f(s);
            alts.iter().for_each(|a| f(&a.rhs));
        }
    }
}

fn fresh(name: &str, ty: Type, next: &mut u32) -> Binder {
    let b = Binder::new(Id::new(name, *next), Mult::Many, ty);
    *next += 1;
    b
}

fn rewrite(e: Expr, plan: &Plan, next: &mut u32) -> Expr {
    let mut go = |e: Expr| rewrite(e, plan, next);
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => e,
        Expr::Con(k, ts, args) => {
            let args: Vec<Expr> = args.into_iter().map(&mut go).collect();
            let Some((dn, fs)) = plan.get(&k) else {
                return Expr::Con(k, ts, args);
            };
            // case a of K# u -> … Con(.., u, ..), innermost for the last field.
            let mut unwrap = Vec::new();
            let mut new_args = Vec::new();
            for (a, f) in args.into_iter().zip(fs) {
                match (f, a) {
                    (None, a) => new_args.push(a),
                    (Some((bk, _, _)), Expr::Con(c, _, mut xs)) if &c == bk && xs.len() == 1 => {
                        new_args.push(xs.pop().unwrap())
                    }
                    (Some((bk, ut, bt)), a) => {
                        let u = fresh("u", ut.clone(), next);
                        new_args.push(Expr::Var(u.id.clone()));
                        unwrap.push((bk.clone(), bt.clone(), u, a));
                    }
                }
            }
            let ty = Type::Con(dn.clone(), ts.clone());
            let mut out = Expr::Con(k, ts, new_args);
            for (bk, bt, u, a) in unwrap.into_iter().rev() {
                let s = fresh("s", bt, next);
                out = Expr::Case(
                    Box::new(a),
                    s,
                    ty.clone(),
                    vec![Alt {
                        con: AltCon::Con(bk, vec![u]),
                        rhs: out,
                    }],
                );
            }
            out
        }
        Expr::Prim(op, ts, a) => Expr::Prim(op, ts, a.into_iter().map(go).collect()),
        Expr::Extern(n, a) => Expr::Extern(n, a.into_iter().map(go).collect()),
        Expr::App(x, y) => Expr::App(Box::new(go(*x)), Box::new(go(*y))),
        Expr::TyApp(x, t) => Expr::TyApp(Box::new(go(*x)), t),
        Expr::Lam(b, x) => Expr::Lam(b, Box::new(go(*x))),
        Expr::TyLam(v, k, x) => Expr::TyLam(v, k, Box::new(go(*x))),
        Expr::Cast(x, t) => Expr::Cast(Box::new(go(*x)), t),
        Expr::LetStrict(b, r, body) => Expr::LetStrict(b, Box::new(go(*r)), Box::new(go(*body))),
        Expr::Let(Bind::NonRec(b, r), body) => {
            Expr::Let(Bind::NonRec(b, Box::new(go(*r))), Box::new(go(*body)))
        }
        Expr::Let(Bind::Rec(bs), body) => Expr::Let(
            Bind::Rec(bs.into_iter().map(|(b, r)| (b, go(r))).collect()),
            Box::new(go(*body)),
        ),
        Expr::Case(s, b, t, alts) => {
            let s = go(*s);
            let alts = alts
                .into_iter()
                .map(|a| {
                    let rhs = rewrite(a.rhs, plan, next);
                    match a.con {
                        AltCon::Con(k, xs) if plan.contains_key(&k) => {
                            let fs = &plan[&k].1;
                            let mut rhs = rhs;
                            let mut ys = Vec::new();
                            for (x, f) in xs.into_iter().zip(fs) {
                                match f {
                                    None => ys.push(x),
                                    Some((bk, ut, _)) => {
                                        let u = fresh("u", ut.clone(), next);
                                        rhs = Expr::Let(
                                            Bind::NonRec(
                                                x,
                                                Box::new(Expr::Con(
                                                    bk.clone(),
                                                    Vec::new(),
                                                    vec![Expr::Var(u.id.clone())],
                                                )),
                                            ),
                                            Box::new(rhs),
                                        );
                                        ys.push(u);
                                    }
                                }
                            }
                            Alt {
                                con: AltCon::Con(k, ys),
                                rhs,
                            }
                        }
                        con => Alt { con, rhs },
                    }
                })
                .collect();
            Expr::Case(Box::new(s), b, t, alts)
        }
    }
}
