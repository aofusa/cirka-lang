//! State-token arity for IO (BUG-0011, 06-ir.md §1.6).
//!
//! `IO a` is a newtype over `Unit# -> a`, and so are the newtypes over it
//! (`Borrow r a`). A function returning one, `f : τ̄ -> IO a`, builds the
//! action as a closure that its caller then runs: a monadic loop allocates
//! a tree of closures per iteration. This pass gives such functions a
//! worker taking the state token,
//!
//! ```text
//! f$io = λx̄ w. run(body, w)        run(e, w) = (cast e (Unit# -> a)) w
//! f    = λx̄. cast (λw. f$io x̄ w) R   (pushed into lets and alternatives)
//! ```
//!
//! and the simplifier inlines the wrapper, so `f x̄` run with `w` becomes
//! the call `f$io x̄ w`. Like GHC's "state hack", this assumes an action is
//! run once: the work of `f` before it returns the action is repeated each
//! time a shared partial application is run.

use std::collections::HashMap;

use crate::opt::map_children;
use crate::ww::split_fun;
use crate::*;

/// The result type `a` when `t` is `IO a` or a newtype over it.
pub(crate) fn io_result(p: &Program, t: &Type) -> Option<Type> {
    let mut t = t.clone();
    for _ in 0..8 {
        match t {
            Type::Fun(_, a, r) if a.alpha_eq(&Type::unit()) => return Some(*r),
            Type::Con(ref n, ref ts) => {
                let d = p.datas.iter().find(|d| d.name == *n)?;
                if !d.newtype || d.ctors.len() != 1 || d.ctors[0].fields.len() != 1 {
                    return None;
                }
                let s: Vec<(TyVar, Type)> = d
                    .params
                    .iter()
                    .map(|(v, _)| v.clone())
                    .zip(ts.iter().cloned())
                    .collect();
                if s.len() != d.params.len() {
                    return None;
                }
                t = d.ctors[0].fields[0].subst_all(&s);
            }
            _ => return None,
        }
    }
    None
}

/// Whether `body` (under the `Λ`s) is the wrapper `λx̄. cast (λw. f$io … w) R`
/// of `f` made by [`io_arity`] (a name alone is not enough: lambda lifting
/// may name an unrelated function `f$io`).
pub(crate) fn is_wrapper(f: &str, body: &Expr) -> bool {
    let mut e = body;
    while let Expr::Lam(_, x) = e {
        e = x;
    }
    let Expr::Cast(x, _) = e else { return false };
    let Expr::Lam(_, call) = &**x else {
        return false;
    };
    let mut h = &**call;
    while let Expr::App(g, _) | Expr::TyApp(g, _) = h {
        h = g;
    }
    matches!(h, Expr::Global(g) if *g == format!("{f}$io"))
}

/// The type after `n` arrows of `t`.
fn result_after(t: &Type, n: usize) -> Option<&Type> {
    let mut t = t;
    for _ in 0..n {
        match t {
            Type::Fun(_, _, r) => t = r,
            _ => return None,
        }
    }
    Some(t)
}

fn io_fun(a: &Type) -> Type {
    Type::Fun(Mult::Many, Box::new(Type::unit()), Box::new(a.clone()))
}

fn fresh(name: &str, ty: Type, next: &mut u32) -> Binder {
    *next += 1;
    Binder::new(Id::new(name, *next), Mult::Many, ty)
}

/// `e w` for `e : R` (an IO-like type with result `a`), pushed into the
/// lets and alternatives of `e`.
fn run(p: &Program, e: Expr, w: &Expr, a: &Type) -> Expr {
    match e {
        Expr::Let(b, body) => Expr::Let(b, Box::new(run(p, *body, w, a))),
        Expr::LetStrict(b, r, body) => Expr::LetStrict(b, r, Box::new(run(p, *body, w, a))),
        Expr::Case(s, b, _, alts) => Expr::Case(
            s,
            b,
            a.clone(),
            alts.into_iter()
                .map(|alt| Alt {
                    con: alt.con,
                    rhs: run(p, alt.rhs, w, a),
                })
                .collect(),
        ),
        Expr::Con(k, _, mut args)
            if args.len() == 1 && p.ctor(&k).is_some_and(|(d, _)| d.newtype) =>
        {
            run(p, args.pop().unwrap(), w, a)
        }
        Expr::Cast(x, _) => run(p, *x, w, a),
        Expr::Panic(m, _) => Expr::Panic(m, a.clone()),
        e => Expr::App(
            Box::new(Expr::Cast(Box::new(e), io_fun(a))),
            Box::new(w.clone()),
        ),
    }
}

/// `λx̄. cast (λw. worker x̄ w) r` with fresh binders like `xs`.
fn wrapper(worker: Expr, xs: &[Binder], r: &Type, next: &mut u32) -> Expr {
    let ys: Vec<Binder> = xs
        .iter()
        .map(|x| fresh(&x.id.name, x.ty.clone(), next))
        .collect();
    let w = fresh("w", Type::unit(), next);
    let call = Expr::app(
        worker,
        ys.iter()
            .map(|y| Expr::var(&y.id))
            .chain([Expr::var(&w.id)]),
    );
    let mut e = Expr::Cast(Box::new(Expr::Lam(w, Box::new(call))), r.clone());
    for y in ys.into_iter().rev() {
        e = Expr::Lam(y, Box::new(e));
    }
    e
}

/// The worker body `λx̄ w. run(body, w)`.
fn worker_body(p: &Program, xs: &[Binder], body: Expr, a: &Type, next: &mut u32) -> Expr {
    let w = fresh("w", Type::unit(), next);
    let wv = Expr::var(&w.id);
    let mut e = Expr::Lam(w, Box::new(run(p, body, &wv, a)));
    for x in xs.iter().rev() {
        e = Expr::Lam(x.clone(), Box::new(e));
    }
    e
}

/// Splits the IO-returning functions (top-level and local `letrec`s).
/// Returns the number split.
pub fn io_arity(p: &mut Program, next: &mut u32) -> usize {
    let mut n = 0;
    let mut new = Vec::new();
    for i in 0..p.binds.len() {
        let b = &p.binds[i];
        if b.name == "main" || b.name.ends_with("$io") {
            continue;
        }
        let (tvs, xs, body) = split_fun(&b.expr);
        if xs.is_empty() || xs.iter().any(|x| x.mult != Mult::Many) {
            continue;
        }
        // The signature's type variables are those of the `Λ`s.
        let mut ty = b.ty.clone();
        let mut k = 0;
        while let Type::Forall(v, _, t) = ty {
            ty = match tvs.get(k) {
                Some((w, _)) => t.subst(&v, &Type::Var(w.clone())),
                None => *t,
            };
            k += 1;
        }
        if k != tvs.len() {
            continue;
        }
        let Some(r) = result_after(&ty, xs.len()).cloned() else {
            continue;
        };
        let Some(a) = io_result(p, &r) else { continue };
        let body = body.clone();
        let name = format!("{}$io", b.name);
        let span = b.span;
        let mut wty = a.clone();
        wty = Type::Fun(Mult::Many, Box::new(Type::unit()), Box::new(wty));
        for x in xs.iter().rev() {
            wty = Type::Fun(x.mult, Box::new(x.ty.clone()), Box::new(wty));
        }
        for (v, kd) in tvs.iter().rev() {
            wty = Type::Forall(v.clone(), kd.clone(), Box::new(wty));
        }
        let mut wexpr = worker_body(p, &xs, body, &a, next);
        let mut head = Expr::Global(name.clone());
        for (v, _) in &tvs {
            head = Expr::TyApp(Box::new(head), Type::Var(v.clone()));
        }
        let mut wrap = wrapper(head, &xs, &r, next);
        for (v, kd) in tvs.iter().rev() {
            wexpr = Expr::TyLam(v.clone(), kd.clone(), Box::new(wexpr));
            wrap = Expr::TyLam(v.clone(), kd.clone(), Box::new(wrap));
        }
        p.binds[i].expr = wrap;
        new.push(TopBind {
            name,
            ty: wty,
            expr: wexpr,
            span,
        });
        n += 1;
    }
    p.binds.extend(new);
    for i in 0..p.binds.len() {
        let e = std::mem::replace(&mut p.binds[i].expr, Expr::Lit(Lit::Unit));
        p.binds[i].expr = local(p, e, next, &mut n);
    }
    n
}

/// A local function split by [`io_arity`]: its worker, parameter types,
/// result type and IO result.
type Split = (Id, Vec<Type>, Type, Type);

/// Splits the IO-returning functions of the local `letrec`s in `e`.
fn local(p: &Program, e: Expr, next: &mut u32, n: &mut usize) -> Expr {
    let e = map_children(e, &mut |x| local(p, x, next, n));
    let Expr::Let(Bind::Rec(bs), body) = e else {
        return e;
    };
    let mut splits: HashMap<u32, Split> = HashMap::new();
    for (b, rhs) in &bs {
        let (tvs, xs, _) = split_fun(rhs);
        if !tvs.is_empty()
            || xs.is_empty()
            || b.mult != Mult::Many
            || xs.iter().any(|x| x.mult != Mult::Many)
        {
            continue;
        }
        let Some(r) = result_after(&b.ty, xs.len()) else {
            continue;
        };
        let Some(a) = io_result(p, r) else { continue };
        *next += 1;
        let w = Id::new(format!("{}$io", b.id.name), *next);
        let ps = xs.iter().map(|x| x.ty.clone()).collect();
        splits.insert(b.id.id, (w, ps, r.clone(), a));
    }
    if splits.is_empty() {
        return Expr::Let(Bind::Rec(bs), body);
    }
    *n += splits.len();
    let mut out = Vec::new();
    for (b, rhs) in bs {
        let rhs = calls(rhs, &splits, next);
        let Some((w, _, r, a)) = splits.get(&b.id.id) else {
            out.push((b, rhs));
            continue;
        };
        let (_, xs, body) = split_fun(&rhs);
        let xs = xs.clone();
        let wexpr = worker_body(p, &xs, body.clone(), a, next);
        let mut wty = Type::Fun(Mult::Many, Box::new(Type::unit()), Box::new(a.clone()));
        for x in xs.iter().rev() {
            wty = Type::Fun(x.mult, Box::new(x.ty.clone()), Box::new(wty));
        }
        let wrap = wrapper(Expr::var(w), &xs, r, next);
        out.push((Binder::new(w.clone(), Mult::Many, wty), wexpr));
        out.push((b, wrap));
    }
    let body = calls(*body, &splits, next);
    Expr::Let(Bind::Rec(out), Box::new(body))
}

/// Replaces the saturated calls `f ā` of split local functions by
/// `cast (λw. f$io ā w) R` (non-atomic arguments bound by `let` first).
fn calls(e: Expr, splits: &HashMap<u32, Split>, next: &mut u32) -> Expr {
    if let Expr::App(..) = e {
        let mut args = Vec::new();
        let mut h = e;
        while let Expr::App(f, a) = h {
            args.push(*a);
            h = *f;
        }
        args.reverse();
        let args: Vec<Expr> = args.into_iter().map(|a| calls(a, splits, next)).collect();
        if let Expr::Var(f) = &h
            && let Some((w, ps, r, _)) = splits.get(&f.id)
            && args.len() == ps.len()
        {
            let mut lets = Vec::new();
            let mut atoms = Vec::new();
            for (a, t) in args.into_iter().zip(ps) {
                if a.is_atom() {
                    atoms.push(a);
                } else {
                    let x = fresh("a", t.clone(), next);
                    atoms.push(Expr::var(&x.id));
                    lets.push((x, a));
                }
            }
            let wv = fresh("w", Type::unit(), next);
            let call = Expr::app(Expr::var(w), atoms.into_iter().chain([Expr::var(&wv.id)]));
            let mut e = Expr::Cast(Box::new(Expr::Lam(wv, Box::new(call))), r.clone());
            for (x, a) in lets.into_iter().rev() {
                e = Expr::Let(Bind::NonRec(x, Box::new(a)), Box::new(e));
            }
            return e;
        }
        let h = calls(h, splits, next);
        return Expr::app(h, args);
    }
    map_children(e, &mut |x| calls(x, splits, next))
}
