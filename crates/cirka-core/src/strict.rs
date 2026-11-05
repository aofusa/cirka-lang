//! Passes that remove thunks (FEAT-0112, 04-memory-model.md §4):
//!
//! - [`float_dict_fields`]: fields of top-level constructor values
//!   (type class dictionaries) become globals of their own, so that every
//!   field is atomic and method selection is resolved by
//!   case-of-known-constructor.
//! - [`lambda_lift`]: local functions that are only called saturated
//!   become top-level functions taking their free variables as extra
//!   parameters, so that the strictness analysis and worker/wrapper reach
//!   them (the typical `go` loop of a `where`).
//! - [`strictify`]: uses the strictness analysis of [`crate::ww`] to
//!   evaluate eagerly what is certainly evaluated later: a lazy `let` whose
//!   body is strict in the variable becomes `let!`, and a non-atomic
//!   argument in a strict parameter position of a known function is bound
//!   with `let!` instead of being passed as a thunk.
//!
//! Like GHC with optimization, divergence is treated as strict in every
//! variable (imprecise exceptions): when two errors race, which one is
//! reported may change.

use crate::expr::*;
use crate::opt::{free_vars, freshen, map_children, replace_var, walk};
use crate::ty::*;
use crate::ww::{Analysis, split_fun, strictness};
use std::collections::{BTreeSet, HashMap, HashSet};

/// The type of every binder in `e` (binders are unique).
pub(crate) fn binder_types(e: &Expr) -> HashMap<u32, Binder> {
    let mut m = HashMap::new();
    walk(e, &mut |s| match s {
        Expr::Lam(b, _) | Expr::LetStrict(b, _, _) | Expr::Let(Bind::NonRec(b, _), _) => {
            m.insert(b.id.id, b.clone());
        }
        Expr::Let(Bind::Rec(bs), _) => {
            for (b, _) in bs {
                m.insert(b.id.id, b.clone());
            }
        }
        Expr::Case(_, b, _, alts) => {
            m.insert(b.id.id, b.clone());
            for a in alts {
                if let AltCon::Con(_, fs) = &a.con {
                    for f in fs {
                        m.insert(f.id.id, f.clone());
                    }
                }
            }
        }
        _ => {}
    });
    m
}

/// The type of `e`, given the types of its free variables.
pub(crate) fn type_of(p: &Program, tys: &HashMap<u32, Binder>, e: &Expr) -> Option<Type> {
    Some(match e {
        Expr::Var(x) => tys.get(&x.id)?.ty.clone(),
        Expr::Global(g) => p.bind(g)?.ty.clone(),
        Expr::Lit(l) => l.ty(),
        Expr::Con(k, ts, _) => Type::Con(p.ctor(k)?.0.name.clone(), ts.clone()),
        Expr::App(f, _) => match type_of(p, tys, f)? {
            Type::Fun(_, _, r) => *r,
            _ => return None,
        },
        Expr::TyApp(f, t) => match type_of(p, tys, f)? {
            Type::Forall(v, _, b) => b.subst(&v, t),
            _ => return None,
        },
        Expr::Lam(b, body) => Type::fun(b.mult, b.ty.clone(), type_of(p, tys, body)?),
        Expr::TyLam(v, k, body) => {
            Type::Forall(v.clone(), k.clone(), Box::new(type_of(p, tys, body)?))
        }
        Expr::Let(_, body) | Expr::LetStrict(_, _, body) => type_of(p, tys, body)?,
        Expr::Case(_, _, t, _) | Expr::Cast(_, t) | Expr::Panic(_, t) => t.clone(),
        Expr::Prim(op, ts, _) => op.signature(ts).1,
        Expr::Extern(n, _) => p.extern_decl(n)?.ret.clone(),
    })
}

/// Free type variables of all type annotations in `e` (minus those bound
/// by `Λ`s inside `e`).
fn free_tyvars(e: &Expr, out: &mut Vec<TyVar>) {
    let mut all = Vec::new();
    let mut bound = Vec::new();
    let ty = |t: &Type, all: &mut Vec<TyVar>| t.free_vars(all);
    walk(e, &mut |s| match s {
        Expr::Lam(b, _) | Expr::LetStrict(b, _, _) | Expr::Let(Bind::NonRec(b, _), _) => {
            ty(&b.ty, &mut all)
        }
        Expr::Let(Bind::Rec(bs), _) => bs.iter().for_each(|(b, _)| ty(&b.ty, &mut all)),
        Expr::Case(_, b, t, alts) => {
            ty(&b.ty, &mut all);
            ty(t, &mut all);
            for a in alts {
                if let AltCon::Con(_, fs) = &a.con {
                    fs.iter().for_each(|f| ty(&f.ty, &mut all));
                }
            }
        }
        Expr::TyApp(_, t) | Expr::Cast(_, t) | Expr::Panic(_, t) => ty(t, &mut all),
        Expr::Con(_, ts, _) | Expr::Prim(_, ts, _) => ts.iter().for_each(|t| ty(t, &mut all)),
        Expr::TyLam(v, _, _) => bound.push(v.clone()),
        _ => {}
    });
    for v in all {
        if !bound.contains(&v) && !out.contains(&v) {
            out.push(v);
        }
    }
}

// ---- dictionary fields ----

/// Makes every field of a top-level constructor value atomic by moving
/// non-atomic fields to globals `name$fN`. Returns the number moved.
pub fn float_dict_fields(p: &mut Program) -> usize {
    let mut new = Vec::new();
    for i in 0..p.binds.len() {
        let Expr::Con(k, ts, args) = &p.binds[i].expr else {
            continue;
        };
        if args.iter().all(|a| a.is_atom()) {
            continue;
        }
        let Some((d, ci)) = p.ctor(k) else { continue };
        let s: Vec<(TyVar, Type)> = d
            .params
            .iter()
            .map(|(v, _)| v.clone())
            .zip(ts.iter().cloned())
            .collect();
        let fields: Vec<Type> = d.ctors[ci].fields.iter().map(|f| f.subst_all(&s)).collect();
        let name = p.binds[i].name.clone();
        let span = p.binds[i].span;
        let mut atoms = Vec::new();
        for (j, (a, ft)) in args.iter().zip(fields).enumerate() {
            if a.is_atom() {
                atoms.push(a.clone());
                continue;
            }
            let g = format!("{name}$f{j}");
            atoms.push(Expr::Global(g.clone()));
            new.push(TopBind {
                name: g,
                ty: ft,
                expr: a.clone(),
                span,
            });
        }
        p.binds[i].expr = Expr::Con(k.clone(), ts.clone(), atoms);
    }
    let n = new.len();
    p.binds.extend(new);
    n
}

// ---- lambda lifting ----

/// Free variables a lifted function may take as extra parameters.
const MAX_LIFT_PARAMS: usize = 5;

/// Lifts local functions that are only called saturated to top level.
/// Returns the number of lifted functions.
pub fn lambda_lift(p: &mut Program, next: &mut u32) -> usize {
    let mut names: HashSet<String> = p.binds.iter().map(|b| b.name.clone()).collect();
    let mut lifted = 0;
    let mut i = 0;
    while i < p.binds.len() {
        let e = p.binds[i].expr.clone();
        let mut l = Lifter {
            tys: binder_types(&e),
            scope: Vec::new(),
            owner: p.binds[i].name.clone(),
            names: &mut names,
            out: Vec::new(),
        };
        let e = l.expr(e);
        let span = p.binds[i].span;
        let out = std::mem::take(&mut l.out);
        if !out.is_empty() {
            lifted += out.len();
            p.binds[i].expr = e;
            for (name, ty, expr) in out {
                let expr = freshen(&expr, next);
                p.binds.push(TopBind {
                    name,
                    ty,
                    expr,
                    span,
                });
            }
        }
        i += 1;
    }
    lifted
}

struct Lifter<'a> {
    tys: HashMap<u32, Binder>,
    /// Type variables bound by enclosing `Λ`s.
    scope: Vec<(TyVar, Kind)>,
    owner: String,
    names: &'a mut HashSet<String>,
    out: Vec<(String, Type, Expr)>,
}

impl Lifter<'_> {
    fn expr(&mut self, e: Expr) -> Expr {
        match e {
            Expr::TyLam(v, k, b) => {
                self.scope.push((v.clone(), k.clone()));
                let b = self.expr(*b);
                self.scope.pop();
                Expr::TyLam(v, k, Box::new(b))
            }
            Expr::Let(Bind::NonRec(b, r), body) => {
                let r = self.expr(*r);
                let body = self.expr(*body);
                self.lift(vec![(b, r)], body, false)
            }
            Expr::Let(Bind::Rec(bs), body) => {
                let bs = bs.into_iter().map(|(b, r)| (b, self.expr(r))).collect();
                let body = self.expr(*body);
                self.lift(bs, body, true)
            }
            e => map_children(e, &mut |c| self.expr(c)),
        }
    }

    /// `let(rec) bs in body`, with the group lifted when possible.
    fn lift(&mut self, bs: Vec<(Binder, Expr)>, body: Expr, rec: bool) -> Expr {
        let keep = |bs: Vec<(Binder, Expr)>, body: Expr| {
            if rec {
                Expr::Let(Bind::Rec(bs), Box::new(body))
            } else {
                let (b, r) = bs.into_iter().next().unwrap();
                Expr::Let(Bind::NonRec(b, Box::new(r)), Box::new(body))
            }
        };
        let mut arity = HashMap::new();
        for (b, r) in &bs {
            let (_, xs, _) = split_fun(r);
            if xs.is_empty() || b.mult != Mult::Many {
                return keep(bs, body);
            }
            arity.insert(b.id.id, xs.len());
        }
        let mut fvs = BTreeSet::new();
        for (_, r) in &bs {
            fvs.extend(free_vars(r));
        }
        for b in arity.keys() {
            fvs.remove(b);
        }
        if fvs.len() > MAX_LIFT_PARAMS {
            return keep(bs, body);
        }
        let mut params = Vec::new();
        for v in &fvs {
            match self.tys.get(v) {
                Some(b) if b.mult == Mult::Many => params.push(b.clone()),
                _ => return keep(bs, body),
            }
        }
        if !bs.iter().all(|(_, r)| saturated(r, &arity)) || !saturated(&body, &arity) {
            return keep(bs, body);
        }
        let mut ftv = Vec::new();
        for (b, r) in &bs {
            b.ty.free_vars(&mut ftv);
            free_tyvars(r, &mut ftv);
        }
        for b in &params {
            b.ty.free_vars(&mut ftv);
        }
        if !ftv.iter().all(|v| self.scope.iter().any(|(s, _)| s == v)) {
            return keep(bs, body);
        }
        let tvs: Vec<(TyVar, Kind)> = self
            .scope
            .iter()
            .filter(|(v, _)| ftv.contains(v))
            .cloned()
            .collect();
        // The call replacing each function of the group.
        let mut calls = Vec::new();
        for (b, _) in &bs {
            let mut name = format!("{}${}", self.owner, b.id.name);
            let mut k = 1;
            while self.names.contains(&name) {
                k += 1;
                name = format!("{}${}{k}", self.owner, b.id.name);
            }
            self.names.insert(name.clone());
            let mut call = Expr::Global(name.clone());
            for (v, _) in &tvs {
                call = Expr::TyApp(Box::new(call), Type::Var(v.clone()));
            }
            for x in &params {
                call = Expr::App(Box::new(call), Box::new(Expr::Var(x.id.clone())));
            }
            calls.push((b.id.id, name, call));
        }
        let subst = |mut e: Expr| {
            for (x, _, c) in &calls {
                e = replace_var(e, *x, c);
            }
            e
        };
        for ((b, r), (_, name, _)) in bs.iter().zip(&calls) {
            let mut e = subst(r.clone());
            let mut ty = b.ty.clone();
            for x in params.iter().rev() {
                e = Expr::Lam(x.clone(), Box::new(e));
                ty = Type::fun(Mult::Many, x.ty.clone(), ty);
            }
            for (v, k) in tvs.iter().rev() {
                e = Expr::TyLam(v.clone(), k.clone(), Box::new(e));
                ty = Type::Forall(v.clone(), k.clone(), Box::new(ty));
            }
            self.out.push((name.clone(), ty, e));
        }
        subst(body)
    }
}

/// Every occurrence in `e` of a function of `arity` is the head of a call
/// with at least that many arguments.
fn saturated(e: &Expr, arity: &HashMap<u32, usize>) -> bool {
    let mut args = Vec::new();
    let mut h = e;
    loop {
        match h {
            Expr::App(f, a) => {
                args.push(&**a);
                h = f;
            }
            Expr::TyApp(f, _) => h = f,
            _ => break,
        }
    }
    if let Expr::Var(x) = h
        && let Some(n) = arity.get(&x.id)
    {
        return args.len() >= *n && args.iter().all(|a| saturated(a, arity));
    }
    if !std::ptr::eq(h, e) {
        // A chain of applications whose head is not a lifted function.
        return saturated(h, arity) && args.iter().all(|a| saturated(a, arity));
    }
    match e {
        Expr::Var(x) => !arity.contains_key(&x.id),
        Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => true,
        Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => {
            a.iter().all(|x| saturated(x, arity))
        }
        Expr::App(..) | Expr::TyApp(..) => unreachable!("handled above"),
        Expr::TyLam(_, _, x) | Expr::Cast(x, _) | Expr::Lam(_, x) => saturated(x, arity),
        Expr::Let(Bind::NonRec(_, r), b) | Expr::LetStrict(_, r, b) => {
            saturated(r, arity) && saturated(b, arity)
        }
        Expr::Let(Bind::Rec(bs), b) => {
            bs.iter().all(|(_, r)| saturated(r, arity)) && saturated(b, arity)
        }
        Expr::Case(s, _, _, alts) => {
            saturated(s, arity) && alts.iter().all(|a| saturated(&a.rhs, arity))
        }
    }
}

// ---- let-to-let! and strict arguments ----

/// Evaluates eagerly what is certainly evaluated: lazy `let`s whose body
/// is strict in the variable, and non-atomic arguments in strict
/// parameter positions of top-level functions. Returns the number of
/// thunks avoided.
pub fn strictify(p: &mut Program, next: &mut u32) -> usize {
    let a = Analysis {
        p,
        strict: strictness(p),
    };
    let mut out = Vec::new();
    let mut n = 0;
    for b in &p.binds {
        let mut s = Strictifier {
            a: &a,
            tys: binder_types(&b.expr),
            next: *next,
            count: 0,
        };
        out.push(s.expr(b.expr.clone()));
        *next = s.next;
        n += s.count;
    }
    for (b, e) in p.binds.iter_mut().zip(out) {
        b.expr = e;
    }
    n
}

struct Strictifier<'a, 'p> {
    a: &'a Analysis<'p>,
    tys: HashMap<u32, Binder>,
    next: u32,
    count: usize,
}

impl Strictifier<'_, '_> {
    fn expr(&mut self, e: Expr) -> Expr {
        match e {
            Expr::Let(Bind::NonRec(b, r), body) => {
                let r = self.expr(*r);
                let body = self.expr(*body);
                let forced = self.a.expr(&body).is_none_or(|s| s.contains(&b.id.id));
                if forced && b.mult == Mult::Many && !r.is_value() && !b.ty.is_unboxed() {
                    self.count += 1;
                    Expr::LetStrict(b, Box::new(r), Box::new(body))
                } else {
                    Expr::Let(Bind::NonRec(b, Box::new(r)), Box::new(body))
                }
            }
            Expr::App(..) => self.app(e),
            e => map_children(e, &mut |c| self.expr(c)),
        }
    }

    /// A call: non-atomic arguments in strict positions of a known
    /// top-level function are evaluated before the call.
    fn app(&mut self, e: Expr) -> Expr {
        let (h, args) = e.collect_args();
        let mut g = h;
        while let Expr::TyApp(x, _) = g {
            g = x;
        }
        let strict = match g {
            Expr::Global(name) => match self.a.strict.get(name) {
                Some((n, st)) if args.len() >= *n => st.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let params = if strict.iter().any(|s| *s) {
            type_of(self.a.p, &self.tys, h)
                .map(|t| t.split_fun().0)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let h = h.clone();
        let args: Vec<Expr> = args.into_iter().cloned().collect();
        let h = self.expr(h);
        let mut binds = Vec::new();
        let mut call = h;
        for (i, a) in args.into_iter().enumerate() {
            let a = self.expr(a);
            let eager = strict.get(i).copied().unwrap_or(false) && !a.is_atom() && !a.is_value();
            let a = match params.get(i) {
                Some((Mult::Many, t)) if eager && !t.is_unboxed() && no_forall(t) => {
                    let x = Binder::new(Id::new("s", self.next), Mult::Many, t.clone());
                    self.next += 1;
                    self.tys.insert(x.id.id, x.clone());
                    self.count += 1;
                    binds.push((x.clone(), a));
                    Expr::Var(x.id)
                }
                _ => a,
            };
            call = Expr::App(Box::new(call), Box::new(a));
        }
        for (x, a) in binds.into_iter().rev() {
            call = Expr::LetStrict(x, Box::new(a), Box::new(call));
        }
        call
    }
}

/// Parameter types after the leading `Λ`s were applied (a polymorphic
/// argument is passed as is).
fn no_forall(t: &Type) -> bool {
    !matches!(t, Type::Forall(..))
}

// ---- float-in ----

/// Moves lazy `let`s down past `let!`s and into the only `case`
/// alternative that uses them, so that a loop does not allocate (box or
/// thunk) on every iteration what only its exit uses. Returns the number of
/// bindings moved into an alternative.
pub fn float_in(p: &mut Program) -> usize {
    let mut n = 0;
    for b in &mut p.binds {
        let e = std::mem::replace(&mut b.expr, Expr::Lit(Lit::Unit));
        b.expr = float_expr(e, &mut n);
    }
    n
}

fn float_expr(e: Expr, n: &mut usize) -> Expr {
    match e {
        Expr::Let(Bind::NonRec(b, r), body) if b.mult == Mult::Many => {
            let r = float_expr(*r, n);
            let body = float_expr(*body, n);
            sink(b, r, body, n)
        }
        e => map_children(e, &mut |c| float_expr(c, n)),
    }
}

fn mentions(e: &Expr, x: u32) -> bool {
    let mut found = false;
    walk(e, &mut |s| {
        if let Expr::Var(v) = s
            && v.id == x
        {
            found = true;
        }
    });
    found
}

/// `let b = r in body` with the binding moved as far down as possible.
fn sink(b: Binder, r: Expr, body: Expr, n: &mut usize) -> Expr {
    let x = b.id.id;
    let keep = |b, r, body| Expr::Let(Bind::NonRec(b, Box::new(r)), Box::new(body));
    match body {
        Expr::LetStrict(b2, r2, rest) if !mentions(&r2, x) => {
            Expr::LetStrict(b2, r2, Box::new(sink(b, r, *rest, n)))
        }
        Expr::Case(s, cb, t, alts) if !mentions(&s, x) => {
            let users: Vec<usize> = (0..alts.len())
                .filter(|&i| mentions(&alts[i].rhs, x))
                .collect();
            if users.len() != 1 {
                return keep(b, r, Expr::Case(s, cb, t, alts));
            }
            *n += 1;
            let i = users[0];
            let alts = alts
                .into_iter()
                .enumerate()
                .map(|(j, a)| {
                    if j == i {
                        let rhs = sink(b.clone(), r.clone(), a.rhs, n);
                        Alt { con: a.con, rhs }
                    } else {
                        a
                    }
                })
                .collect();
            Expr::Case(s, cb, t, alts)
        }
        body => keep(b, r, body),
    }
}
