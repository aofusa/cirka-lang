//! Specialization on static function arguments (FEAT-0061).
//!
//! A recursive top-level function that passes one of its function-typed
//! parameters unchanged to every recursive call, and otherwise only calls
//! it, is copied for each call site that passes a known function there: a
//! global, a lambda, or a partial application of a global to atoms (`a i`);
//! the free variables of the function passed become extra parameters.
//!
//! ```text
//! go f n i = … f a b … go f n (i + 1)        -- f is static in go
//! … go (\x y -> e[u]) n 0 …                  -- call site, u free
//! ==>
//! go$f1 u n i = … (\x y -> e[u]) a b … go$f1 u n (i + 1)
//! … go$f1 u n 0 …
//! ```
//!
//! The simplifier then applies the lambda in place, so the arguments and
//! results that crossed the unknown call no longer need boxes (GHC gets the
//! same effect from the static argument transformation and inlining).
//!
//! A polymorphic function (`Λa. λf x. …`) whose recursive calls are at its
//! own type variables is copied for call sites with ground type arguments;
//! the copy is monomorphic (a library fold called with a known function,
//! FEAT-0079).

use crate::expr::*;
use crate::opt::{free_vars, freshen, map_children, replace_var, size, walk};
use crate::strict::binder_types;
use crate::ty::*;
use crate::ww::split_fun;
use std::collections::{HashMap, HashSet};

/// At most this many copies per program.
const MAX_SPECS: usize = 200;
/// Functions larger than this (Core nodes) are not copied.
const MAX_SIZE: usize = 2000;

/// A function with static function parameters.
struct Target {
    /// Type parameters (instantiated by each copy).
    tvs: Vec<(TyVar, Kind)>,
    params: Vec<Binder>,
    /// Indices of the static function parameters.
    stat: Vec<usize>,
}

/// Specializes calls passing known functions to static parameters; returns
/// the number of copies made.
pub fn specialize_fun_args(p: &mut Program, next: &mut u32) -> usize {
    let mut targets = HashMap::new();
    for b in &p.binds {
        if let Some(t) = target(&b.name, &b.ty, &b.expr) {
            targets.insert(b.name.clone(), t);
        }
    }
    if targets.is_empty() {
        return 0;
    }
    let mut s = Sat {
        targets,
        specs: HashMap::new(),
        names: p.binds.iter().map(|b| b.name.clone()).collect(),
        new: Vec::new(),
    };
    let mut i = 0;
    let mut made = 0;
    while i < p.binds.len() {
        let tys = binder_types(&p.binds[i].expr);
        let e = std::mem::replace(&mut p.binds[i].expr, Expr::Lit(Lit::Unit));
        p.binds[i].expr = s.expr(p, &tys, e, next);
        for (name, ty, expr) in std::mem::take(&mut s.new) {
            made += 1;
            let span = p.binds[i].span;
            p.binds.push(TopBind {
                name,
                ty,
                expr,
                span,
            });
        }
        i += 1;
    }
    made
}

/// `h @t1 .. @tn`: the head under type applications and the types.
fn ty_args(e: &Expr) -> (&Expr, Vec<&Type>) {
    let mut ts = Vec::new();
    let mut e = e;
    while let Expr::TyApp(h, t) = e {
        ts.push(t);
        e = h;
    }
    ts.reverse();
    (e, ts)
}

/// The static function parameters of `f` (recursive, and polymorphic only
/// at its own type variables).
fn target(f: &str, ty: &Type, e: &Expr) -> Option<Target> {
    let (tvs, xs, body) = split_fun(e);
    if xs.is_empty() || size(e) > MAX_SIZE {
        return None;
    }
    let own_tys = |ts: &[&Type]| {
        ts.len() == tvs.len()
            && ts
                .iter()
                .zip(&tvs)
                .all(|(t, (v, _))| matches!(t, Type::Var(w) if w == v))
    };
    let n = xs.len();
    let mut cand: Vec<usize> = (0..n)
        .filter(|&i| xs[i].mult == Mult::Many && matches!(xs[i].ty, Type::Fun(..)))
        .collect();
    if cand.is_empty() {
        return None;
    }
    // Every use of `f` is a saturated call, and every use of a candidate is
    // either the head of a call or its own position in a recursive call.
    let mut uses_f = 0;
    let mut calls_f = 0;
    let mut uses = vec![0usize; n];
    let mut allowed = vec![0usize; n];
    let ids: Vec<u32> = xs.iter().map(|x| x.id.id).collect();
    walk(body, &mut |s| match s {
        Expr::Global(g) if g == f => uses_f += 1,
        Expr::Var(v) => {
            if let Some(i) = ids.iter().position(|&x| x == v.id) {
                uses[i] += 1;
            }
        }
        Expr::App(h, _) => {
            if let Expr::Var(v) = &**h
                && let Some(i) = ids.iter().position(|&x| x == v.id)
            {
                allowed[i] += 1;
            }
            let (h, args) = s.collect_args();
            let (h, ts) = ty_args(h);
            if matches!(h, Expr::Global(g) if g == f) && own_tys(&ts) && args.len() == n {
                calls_f += 1;
                for &i in &cand {
                    if matches!(args[i], Expr::Var(v) if v.id == ids[i]) {
                        allowed[i] += 1;
                    } else {
                        allowed[i] = usize::MAX / 2;
                    }
                }
            }
        }
        _ => {}
    });
    if calls_f == 0 || uses_f != calls_f {
        return None;
    }
    cand.retain(|&i| uses[i] == allowed[i]);
    if cand.is_empty() {
        return None;
    }
    // The type must take the `n` parameters (its `∀` variables are
    // instantiated by each copy).
    let mut result = ty;
    while let Type::Forall(_, _, b) = result {
        result = b;
    }
    for _ in 0..n {
        let Type::Fun(_, _, r) = result else {
            return None;
        };
        result = r;
    }
    Some(Target {
        tvs,
        params: xs,
        stat: cand,
    })
}

struct Sat {
    targets: HashMap<String, Target>,
    /// Canonical call pattern → specialized copy.
    specs: HashMap<String, String>,
    names: HashSet<String>,
    new: Vec<(String, Type, Expr)>,
}

/// Placeholder ids for the free variables of a call pattern's key.
const KEY_ID: u32 = u32::MAX - 1024;

impl Sat {
    fn expr(&mut self, p: &Program, tys: &HashMap<u32, Binder>, e: Expr, next: &mut u32) -> Expr {
        let Expr::App(..) = e else {
            return map_children(e, &mut |c| self.expr(p, tys, c, next));
        };
        let mut args = Vec::new();
        let mut h = e;
        while let Expr::App(f, a) = h {
            args.push(*a);
            h = *f;
        }
        args.reverse();
        let args: Vec<Expr> = args
            .into_iter()
            .map(|a| self.expr(p, tys, a, next))
            .collect();
        let (head, ts) = ty_args(&h);
        if let Expr::Global(f) = head {
            let ts: Vec<Type> = ts.into_iter().cloned().collect();
            if let Some(r) = self.call(p, tys, f, &ts, &args, next) {
                return r;
            }
        }
        let h = self.expr(p, tys, h, next);
        Expr::app(h, args)
    }

    /// The specialized call `f$fN fvs args'` replacing `f args`, if any.
    fn call(
        &mut self,
        p: &Program,
        tys: &HashMap<u32, Binder>,
        f: &str,
        targs: &[Type],
        args: &[Expr],
        next: &mut u32,
    ) -> Option<Expr> {
        let t = self.targets.get(f)?;
        let n = t.params.len();
        if args.len() < n || targs.len() != t.tvs.len() || targs.iter().any(has_tyvars) {
            return None;
        }
        let stat: Vec<usize> = t
            .stat
            .iter()
            .copied()
            .filter(|&i| known_fun(p, &args[i], f) && !expr_has_tyvars(&args[i]))
            .collect();
        if stat.is_empty() {
            return None;
        }
        // Free variables of the passed functions, in a fixed order.
        let mut fvs: Vec<u32> = stat
            .iter()
            .flat_map(|&i| free_vars(&args[i]))
            .collect::<HashSet<u32>>()
            .into_iter()
            .collect();
        fvs.sort_unstable();
        let fv_binders: Vec<Binder> = fvs
            .iter()
            .map(|v| tys.get(v).cloned())
            .collect::<Option<Vec<Binder>>>()?;
        if fv_binders
            .iter()
            .any(|b| b.mult != Mult::Many || has_tyvars(&b.ty))
        {
            return None;
        }
        let key = {
            let mut k = format!("{f}|{stat:?}|{targs:?}");
            for &i in &stat {
                let mut a = args[i].clone();
                for (j, v) in fvs.iter().enumerate() {
                    a = replace_var(a, *v, &Expr::Var(Id::new("fv", KEY_ID + j as u32)));
                }
                let mut c = 0;
                k.push_str(&format!("|{:?}", freshen(&a, &mut c)));
            }
            k
        };
        let name = match self.specs.get(&key) {
            Some(n) => n.clone(),
            None => {
                if self.specs.len() >= MAX_SPECS {
                    return None;
                }
                let name = self.copy(p, f, targs, &stat, args, &fvs, &fv_binders, next)?;
                self.specs.insert(key, name.clone());
                name
            }
        };
        let rest = args
            .iter()
            .enumerate()
            .filter(|(i, _)| !stat.contains(i))
            .map(|(_, a)| a.clone());
        let fv_args = fv_binders.iter().map(|b| Expr::var(&b.id));
        Some(Expr::app(Expr::Global(name), fv_args.chain(rest)))
    }

    /// Makes the copy of `f` for the functions `args[stat]`.
    #[allow(clippy::too_many_arguments)]
    fn copy(
        &mut self,
        p: &Program,
        f: &str,
        targs: &[Type],
        stat: &[usize],
        args: &[Expr],
        fvs: &[u32],
        fv_binders: &[Binder],
        next: &mut u32,
    ) -> Option<String> {
        let fb = p.bind(f)?;
        let (tvs, xs, body) = split_fun(&fb.expr);
        // The copy is monomorphic: the type arguments replace the variables.
        let inst = |ty: &Type| {
            tvs.iter()
                .zip(targs)
                .fold(ty.clone(), |ty, ((v, _), a)| ty.subst(v, a))
        };
        let xs: Vec<Binder> = xs
            .iter()
            .map(|x| Binder::new(x.id.clone(), x.mult, inst(&x.ty)))
            .collect();
        let mut body = body.clone();
        for ((v, _), a) in tvs.iter().zip(targs) {
            body = crate::opt::subst_ty(&body, v, a);
        }
        let mut k = 1;
        let mut name = format!("{f}$f{k}");
        while self.names.contains(&name) {
            k += 1;
            name = format!("{f}$f{k}");
        }
        // Fresh parameters for the free variables.
        let new_fvs: Vec<Binder> = fv_binders
            .iter()
            .map(|b| {
                *next += 1;
                Binder::new(Id::new(b.id.name.clone(), *next), Mult::Many, b.ty.clone())
            })
            .collect();
        let fv_vars: Vec<Expr> = new_fvs.iter().map(|b| Expr::var(&b.id)).collect();
        let mut body = rewrite(body, f, &name, xs.len(), stat, &fv_vars);
        for &i in stat {
            let mut a = args[i].clone();
            for (v, nb) in fvs.iter().zip(&new_fvs) {
                a = replace_var(a, *v, &Expr::var(&nb.id));
            }
            body = replace_var(body, xs[i].id.id, &a);
        }
        let kept: Vec<&Binder> = xs
            .iter()
            .enumerate()
            .filter(|(i, _)| !stat.contains(i))
            .map(|(_, x)| x)
            .collect();
        // The result type, from the binding's own `∀`s.
        let mut ty = fb.ty.clone();
        for a in targs {
            let Type::Forall(v, _, b) = ty else {
                return None;
            };
            ty = b.subst(&v, a);
        }
        for _ in 0..xs.len() {
            let Type::Fun(_, _, r) = ty else {
                return None;
            };
            ty = *r;
        }
        let mut e = body;
        for x in kept.iter().rev() {
            ty = Type::Fun(x.mult, Box::new(x.ty.clone()), Box::new(ty));
            e = Expr::Lam((*x).clone(), Box::new(e));
        }
        for b in new_fvs.iter().rev() {
            ty = Type::Fun(Mult::Many, Box::new(b.ty.clone()), Box::new(ty));
            e = Expr::Lam(b.clone(), Box::new(e));
        }
        let e = freshen(&e, next);
        self.names.insert(name.clone());
        self.new.push((name.clone(), ty, e));
        Some(name)
    }
}

fn has_tyvars(t: &Type) -> bool {
    let mut vs = Vec::new();
    t.free_vars(&mut vs);
    !vs.is_empty()
}

/// Whether a type annotation in `e` mentions a type variable (such a
/// function cannot go into a monomorphic copy).
fn expr_has_tyvars(e: &Expr) -> bool {
    let mut found = false;
    walk(e, &mut |s| {
        let b = |b: &Binder| has_tyvars(&b.ty);
        found |= match s {
            Expr::Panic(_, t) | Expr::TyApp(_, t) | Expr::Cast(_, t) => has_tyvars(t),
            Expr::Con(_, ts, _) | Expr::Prim(_, ts, _) => ts.iter().any(has_tyvars),
            Expr::Lam(x, _) | Expr::LetStrict(x, _, _) | Expr::Let(Bind::NonRec(x, _), _) => b(x),
            Expr::Let(Bind::Rec(bs), _) => bs.iter().any(|(x, _)| b(x)),
            Expr::TyLam(..) => true,
            Expr::Case(_, x, t, alts) => {
                b(x) || has_tyvars(t)
                    || alts.iter().any(|a| match &a.con {
                        AltCon::Con(_, fs) => fs.iter().any(b),
                        _ => false,
                    })
            }
            _ => false,
        };
    });
    found
}

/// Whether `a` is a function worth specializing on: a global function, a
/// lambda (not mentioning `f`, which would specialize without end), or a
/// partial application of a global function to atoms (`a i`, in effect
/// `\j -> a i j`: applying it does no work until it is saturated).
fn known_fun(p: &Program, a: &Expr, f: &str) -> bool {
    match a {
        Expr::Global(g) => g != f && p.bind(g).is_some_and(|b| matches!(b.expr, Expr::Lam(..))),
        Expr::App(..) => {
            let (h, args) = a.collect_args();
            let (h, _) = ty_args(h);
            matches!(h, Expr::Global(g) if g != f && p.bind(g).is_some_and(|b| {
                let (_, xs, _) = split_fun(&b.expr);
                xs.len() > args.len()
            })) && args.iter().all(|x| x.is_atom())
        }
        Expr::Lam(..) => {
            let mut mentions = false;
            walk(a, &mut |s| {
                if matches!(s, Expr::Global(g) if g == f) {
                    mentions = true;
                }
            });
            !mentions
        }
        _ => false,
    }
}

/// Rewrites the recursive calls `f args` of the copy into
/// `new fvs args'` (dropping the static arguments).
fn rewrite(e: Expr, f: &str, new: &str, n: usize, stat: &[usize], fvs: &[Expr]) -> Expr {
    let Expr::App(..) = e else {
        return map_children(e, &mut |c| rewrite(c, f, new, n, stat, fvs));
    };
    let mut args = Vec::new();
    let mut h = e;
    while let Expr::App(g, a) = h {
        args.push(*a);
        h = *g;
    }
    args.reverse();
    let args: Vec<Expr> = args
        .into_iter()
        .map(|a| rewrite(a, f, new, n, stat, fvs))
        .collect();
    // Recursive calls are at the copy's (now ground) type arguments.
    let (head, _) = ty_args(&h);
    if matches!(head, Expr::Global(g) if g == f) && args.len() >= n {
        let rest = args
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !stat.contains(i))
            .map(|(_, a)| a);
        return Expr::app(
            Expr::Global(new.to_string()),
            fvs.iter().cloned().chain(rest),
        );
    }
    Expr::app(rewrite(h, f, new, n, stat, fvs), args)
}
