//! Worker/wrapper transformation with unboxing (FEAT-0041, 06-ir.md §1.3).
//!
//! A *box type* is a data type with a single constructor whose single
//! field is strict and unboxed (`Int = I# !Int#`, `Float = F# !Double#`).
//! For a top-level function `f = Λas. λx₁..xₙ. body` the pass
//!
//! - unboxes every parameter of a box type in which `f` is strict
//!   (evaluating `f`'s body to WHNF certainly evaluates the parameter, so
//!   evaluating it before the call does not change the meaning), and
//! - unboxes the result when it is of a box type and every return of the
//!   body builds the box (constructed product result), and
//!
//! splits `f` into a worker `f$w` over the unboxed values and a small
//! wrapper `f`, which the simplifier inlines at call sites:
//!
//! ```text
//! f   = Λas. λx₁..xₙ. case xᵢ of K uᵢ -> K (f$w @as .. uᵢ ..)
//! f$w = Λas. λ.. uᵢ .. . let xᵢ = K uᵢ in ⟦body⟧
//! ```
//!
//! where `⟦body⟧` replaces each return `K e` by `e` (and calls of `f`
//! itself in return position by calls of the worker). The strictness
//! analysis is a greatest fixed point over all top-level functions.

use crate::expr::*;
use crate::ty::*;
use std::collections::{HashMap, HashSet};

/// The strictness of an expression: the variables certainly evaluated
/// when the expression is evaluated to WHNF; `None` when it diverges.
pub(crate) type Strict = Option<HashSet<u32>>;

fn union(a: Strict, b: Strict) -> Strict {
    match (a, b) {
        (Some(mut x), Some(y)) => {
            x.extend(y);
            Some(x)
        }
        _ => None,
    }
}

fn inter(a: Strict, b: Strict) -> Strict {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.intersection(&y).copied().collect()),
        (None, s) | (s, None) => s,
    }
}

fn without(s: Strict, xs: &[u32]) -> Strict {
    s.map(|mut s| {
        for x in xs {
            s.remove(x);
        }
        s
    })
}

/// The parameters of a top-level function: type variables and value
/// binders of its leading `Λ`s and `λ`s, and the body.
pub(crate) fn split_fun(e: &Expr) -> (Vec<(TyVar, Kind)>, Vec<Binder>, &Expr) {
    let mut tvs = Vec::new();
    let mut e = e;
    while let Expr::TyLam(v, k, b) = e {
        tvs.push((v.clone(), k.clone()));
        e = b;
    }
    let mut xs = Vec::new();
    while let Expr::Lam(b, body) = e {
        xs.push(b.clone());
        e = body;
    }
    (tvs, xs, e)
}

pub(crate) struct Analysis<'a> {
    pub(crate) p: &'a Program,
    /// Strict parameter positions of the top-level functions.
    pub(crate) strict: HashMap<String, (usize, Vec<bool>)>,
}

impl Analysis<'_> {
    pub(crate) fn expr(&self, e: &Expr) -> Strict {
        match e {
            Expr::Var(x) => Some(HashSet::from([x.id])),
            Expr::Global(_) | Expr::Lit(_) | Expr::Lam(..) | Expr::TyLam(..) => {
                Some(HashSet::new())
            }
            Expr::Panic(..) => None,
            Expr::TyApp(x, _) | Expr::Cast(x, _) => self.expr(x),
            Expr::App(..) => {
                let (h, args) = e.collect_args();
                let mut h = h;
                while let Expr::TyApp(x, _) = h {
                    h = x;
                }
                match h {
                    Expr::Global(g) => match self.strict.get(g) {
                        Some((n, st)) if args.len() >= *n => {
                            let mut s = Some(HashSet::new());
                            for (a, st) in args.iter().zip(st) {
                                if *st {
                                    s = union(s, self.expr(a));
                                }
                            }
                            s
                        }
                        _ => Some(HashSet::new()),
                    },
                    Expr::Var(x) => Some(HashSet::from([x.id])),
                    _ => Some(HashSet::new()),
                }
            }
            Expr::Con(k, _, args) => {
                let strict = self
                    .p
                    .ctor(k)
                    .map(|(d, i)| d.ctors[i].strict.clone())
                    .unwrap_or_default();
                let mut s = Some(HashSet::new());
                for (a, st) in args.iter().zip(strict) {
                    if st {
                        s = union(s, self.expr(a));
                    }
                }
                s
            }
            Expr::Prim(op, _, args) => {
                let mut s = Some(HashSet::new());
                for (i, a) in args.iter().enumerate() {
                    let lazy = matches!(
                        (op, i),
                        (PrimOp::ArrayNew, 1) | (PrimOp::ArraySet | PrimOp::ArraySetShared, 2)
                    );
                    if !lazy {
                        s = union(s, self.expr(a));
                    }
                }
                s
            }
            Expr::Extern(_, args) => args
                .iter()
                .try_fold(HashSet::new(), |s, a| union(Some(s), self.expr(a))),
            Expr::Let(Bind::NonRec(b, r), body) => {
                let sb = self.expr(body);
                let forced = sb.as_ref().is_none_or(|s| s.contains(&b.id.id));
                let s = without(sb, &[b.id.id]);
                if forced { union(s, self.expr(r)) } else { s }
            }
            Expr::Let(Bind::Rec(bs), body) => {
                let ids: Vec<u32> = bs.iter().map(|(b, _)| b.id.id).collect();
                without(self.expr(body), &ids)
            }
            Expr::LetStrict(b, r, body) => {
                union(self.expr(r), without(self.expr(body), &[b.id.id]))
            }
            Expr::Case(s, b, _, alts) => {
                let mut acc: Strict = None;
                for a in alts {
                    let mut bound = vec![b.id.id];
                    if let AltCon::Con(_, fs) = &a.con {
                        bound.extend(fs.iter().map(|f| f.id.id));
                    }
                    acc = inter(acc, without(self.expr(&a.rhs), &bound));
                }
                union(self.expr(s), acc)
            }
        }
    }
}

/// Strict parameters of every top-level function (greatest fixed point).
pub(crate) fn strictness(p: &Program) -> HashMap<String, (usize, Vec<bool>)> {
    let mut a = Analysis {
        p,
        strict: HashMap::new(),
    };
    for b in &p.binds {
        let (_, xs, _) = split_fun(&b.expr);
        if !xs.is_empty() {
            a.strict
                .insert(b.name.clone(), (xs.len(), vec![true; xs.len()]));
        }
    }
    loop {
        let mut changed = false;
        for b in &p.binds {
            let (_, xs, body) = split_fun(&b.expr);
            if xs.is_empty() {
                continue;
            }
            let s = a.expr(body);
            let st: Vec<bool> = xs
                .iter()
                .map(|x| s.as_ref().is_none_or(|s| s.contains(&x.id.id)))
                .collect();
            let old = &a.strict[&b.name].1;
            // Monotone: a parameter only stops being strict.
            let st: Vec<bool> = st.iter().zip(old).map(|(n, o)| *n && *o).collect();
            if &st != old {
                changed = true;
                a.strict.insert(b.name.clone(), (xs.len(), st));
            }
        }
        if !changed {
            return a.strict;
        }
    }
}

/// The constructor and field type of a box type.
pub(crate) fn box_of(p: &Program, t: &Type) -> Option<(String, Type)> {
    let Type::Con(d, args) = t else { return None };
    if !args.is_empty() {
        return None;
    }
    let d = p.data(d)?;
    if d.linear || d.newtype || d.ctors.len() != 1 {
        return None;
    }
    let k = &d.ctors[0];
    if k.fields.len() != 1 || !k.strict[0] || !k.fields[0].is_unboxed() {
        return None;
    }
    Some((k.name.clone(), k.fields[0].clone()))
}

/// Every return of `e` builds constructor `k` (or diverges, or calls
/// `f` itself with at least `n` arguments).
fn returns_con(e: &Expr, k: &str, f: &str, n: usize, boxed: &HashSet<u32>) -> bool {
    match e {
        Expr::Con(c, _, _) => c == k,
        // A parameter that the worker receives unboxed.
        Expr::Var(x) => boxed.contains(&x.id),
        Expr::Panic(..) => true,
        Expr::Let(_, b) | Expr::LetStrict(_, _, b) => returns_con(b, k, f, n, boxed),
        Expr::Case(_, _, _, alts) => alts.iter().all(|a| returns_con(&a.rhs, k, f, n, boxed)),
        Expr::App(..) => is_self_call(e, f, n),
        _ => false,
    }
}

fn is_self_call(e: &Expr, f: &str, n: usize) -> bool {
    let (h, args) = e.collect_args();
    let mut h = h;
    while let Expr::TyApp(x, _) = h {
        h = x;
    }
    matches!(h, Expr::Global(g) if g == f) && args.len() == n
}

/// What the worker of a function unboxes.
struct Plan {
    name: String,
    worker: String,
    tvs: Vec<(TyVar, Kind)>,
    params: Vec<Binder>,
    /// For each parameter: its box constructor and field type when unboxed.
    unbox: Vec<Option<(String, Type)>>,
    /// The result's box when the result is unboxed (CPR).
    cpr: Option<(String, Type)>,
    res: Type,
}

/// Splits the eligible top-level functions into workers and wrappers.
/// Returns the number of functions split.
pub fn worker_wrapper(p: &mut Program, next: &mut u32) -> usize {
    let strict = strictness(p);
    let mut plans = Vec::new();
    for b in &p.binds {
        if b.name == "main" || b.name.ends_with("$w") {
            continue;
        }
        let (tvs, xs, body) = split_fun(&b.expr);
        if xs.is_empty() || xs.iter().any(|x| x.mult != Mult::Many) {
            continue;
        }
        let Some((_, st)) = strict.get(&b.name) else {
            continue;
        };
        let unbox: Vec<Option<(String, Type)>> = xs
            .iter()
            .zip(st)
            .map(|(x, s)| if *s { box_of(p, &x.ty) } else { None })
            .collect();
        // The result type after the parameters.
        // The signature's type variables are those of the `Λ`s (which
        // `uniquify` may have renamed).
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
        let mut res = &ty;
        let mut ok = true;
        for _ in 0..xs.len() {
            match res {
                Type::Fun(_, _, r) => res = r,
                _ => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let cpr = box_of(p, res).filter(|(k, _)| {
            let boxed: HashSet<u32> = xs
                .iter()
                .zip(&unbox)
                .filter(|(_, u)| u.as_ref().is_some_and(|(c, _)| c == k))
                .map(|(x, _)| x.id.id)
                .collect();
            returns_con(body, k, &b.name, xs.len(), &boxed)
        });
        if cpr.is_none() && unbox.iter().all(Option::is_none) {
            continue;
        }
        plans.push(Plan {
            name: b.name.clone(),
            worker: format!("{}$w", b.name),
            tvs,
            params: xs,
            unbox,
            cpr,
            res: res.clone(),
        });
    }
    let n = plans.len();
    for plan in plans {
        let i = p.binds.iter().position(|b| b.name == plan.name).unwrap();
        let (_, _, body) = split_fun(&p.binds[i].expr);
        let body = body.clone();
        let fresh = |next: &mut u32, name: &str| {
            let id = Id::new(name, *next);
            *next += 1;
            id
        };
        // Worker parameters: the unboxed fields replace the boxes.
        let mut wparams = Vec::new();
        let mut rebox = Vec::new();
        for (x, u) in plan.params.iter().zip(&plan.unbox) {
            match u {
                Some((k, ft)) => {
                    let ub = Binder::new(fresh(next, &x.id.name), Mult::Many, ft.clone());
                    rebox.push((x.clone(), k.clone(), ub.id.clone()));
                    wparams.push(ub);
                }
                None => wparams.push(x.clone()),
            }
        }
        let wres = match &plan.cpr {
            Some((_, ft)) => ft.clone(),
            None => plan.res.clone(),
        };
        let mut wbody = match &plan.cpr {
            Some((_, ft)) => {
                let m: HashMap<u32, Id> =
                    rebox.iter().map(|(x, _, u)| (x.id.id, u.clone())).collect();
                unbox_returns(body, ft, &plan, &m, next)
            }
            None => body,
        };
        for (x, k, u) in rebox.into_iter().rev() {
            wbody = Expr::Let(
                Bind::NonRec(x, Box::new(Expr::Con(k, Vec::new(), vec![Expr::Var(u)]))),
                Box::new(wbody),
            );
        }
        let wty = fun_type(&plan.tvs, &wparams, &wres);
        let wexpr = abstract_fun(&plan.tvs, &wparams, wbody);
        // Wrapper: unbox the arguments, call the worker, box the result.
        let wparams_w: Vec<Binder> = plan
            .params
            .iter()
            .map(|x| Binder::new(fresh(next, &x.id.name), Mult::Many, x.ty.clone()))
            .collect();
        let mut call = Expr::Global(plan.worker.clone());
        for (v, _) in &plan.tvs {
            call = Expr::TyApp(Box::new(call), Type::Var(v.clone()));
        }
        let mut cases = Vec::new();
        let mut args = Vec::new();
        for (x, u) in wparams_w.iter().zip(&plan.unbox) {
            match u {
                Some((k, ft)) => {
                    let ub = Binder::new(fresh(next, &x.id.name), Mult::Many, ft.clone());
                    args.push(Expr::Var(ub.id.clone()));
                    cases.push((x.clone(), k.clone(), ub));
                }
                None => args.push(Expr::Var(x.id.clone())),
            }
        }
        let mut wrap = Expr::app(call, args);
        if let Some((k, _)) = &plan.cpr {
            wrap = Expr::Con(k.clone(), Vec::new(), vec![wrap]);
        }
        for (x, k, ub) in cases.into_iter().rev() {
            let cb = Binder::new(fresh(next, "s"), Mult::Many, x.ty.clone());
            wrap = Expr::Case(
                Box::new(Expr::Var(x.id.clone())),
                cb,
                plan.res.clone(),
                vec![Alt {
                    con: AltCon::Con(k, vec![ub]),
                    rhs: wrap,
                }],
            );
        }
        let wrapper = abstract_fun(&plan.tvs, &wparams_w, wrap);
        let span = p.binds[i].span;
        p.binds[i].expr = wrapper;
        p.binds.insert(
            i + 1,
            TopBind {
                name: plan.worker.clone(),
                ty: wty,
                expr: wexpr,
                span,
            },
        );
    }
    n
}

fn fun_type(tvs: &[(TyVar, Kind)], xs: &[Binder], res: &Type) -> Type {
    let t = xs
        .iter()
        .rev()
        .fold(res.clone(), |r, x| Type::fun(x.mult, x.ty.clone(), r));
    tvs.iter().rev().fold(t, |t, (v, k)| {
        Type::Forall(v.clone(), k.clone(), Box::new(t))
    })
}

fn abstract_fun(tvs: &[(TyVar, Kind)], xs: &[Binder], body: Expr) -> Expr {
    let e = xs
        .iter()
        .rev()
        .fold(body, |b, x| Expr::Lam(x.clone(), Box::new(b)));
    tvs.iter().rev().fold(e, |b, (v, k)| {
        Expr::TyLam(v.clone(), k.clone(), Box::new(b))
    })
}

/// Replaces the returns `K e` of `e` by `e`, divergence by divergence at
/// the unboxed type, and self-calls by worker calls.
fn unbox_returns(e: Expr, ft: &Type, plan: &Plan, m: &HashMap<u32, Id>, next: &mut u32) -> Expr {
    match e {
        Expr::Var(x) if m.contains_key(&x.id) => Expr::Var(m[&x.id].clone()),
        Expr::Con(_, _, mut args) => args.pop().unwrap(),
        Expr::Panic(m, _) => Expr::Panic(m, ft.clone()),
        Expr::Let(b, body) => Expr::Let(b, Box::new(unbox_returns(*body, ft, plan, m, next))),
        Expr::LetStrict(b, r, body) => {
            Expr::LetStrict(b, r, Box::new(unbox_returns(*body, ft, plan, m, next)))
        }
        Expr::Case(s, b, _, alts) => Expr::Case(
            s,
            b,
            ft.clone(),
            alts.into_iter()
                .map(|a| Alt {
                    con: a.con,
                    rhs: unbox_returns(a.rhs, ft, plan, m, next),
                })
                .collect(),
        ),
        e => {
            // A self-call `f @ts a₁..aₙ`: call the worker, unboxing the
            // arguments it takes unboxed.
            let (h, args) = e.collect_args();
            let mut ty_args = Vec::new();
            let mut h = h;
            while let Expr::TyApp(x, t) = h {
                ty_args.push(t.clone());
                h = x;
            }
            ty_args.reverse();
            let args: Vec<Expr> = args.into_iter().cloned().collect();
            let mut call = Expr::Global(plan.worker.clone());
            for t in ty_args {
                call = Expr::TyApp(Box::new(call), t);
            }
            let mut binds = Vec::new();
            let mut cargs = Vec::new();
            for ((a, u), x) in args.into_iter().zip(&plan.unbox).zip(&plan.params) {
                match u {
                    Some((bk, bt)) => {
                        let ub = Binder::new(Id::new("u", *next), Mult::Many, bt.clone());
                        *next += 1;
                        cargs.push(Expr::Var(ub.id.clone()));
                        binds.push((a, bk.clone(), ub, x.ty.clone()));
                    }
                    None => cargs.push(a),
                }
            }
            let mut out = Expr::app(call, cargs);
            for (a, bk, ub, bty) in binds.into_iter().rev() {
                let cb = Binder::new(Id::new("s", *next), Mult::Many, bty);
                *next += 1;
                out = Expr::Case(
                    Box::new(a),
                    cb,
                    ft.clone(),
                    vec![Alt {
                        con: AltCon::Con(bk, vec![ub]),
                        rhs: out,
                    }],
                );
            }
            out
        }
    }
}
