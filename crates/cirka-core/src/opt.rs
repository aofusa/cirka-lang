//! Core optimization passes (FEAT-0033, 06-ir.md §1.3).
//!
//! Pipeline: `uniquify` (every binder gets a program-wide unique id, which
//! makes substitution capture-free) → `simplify` iterated to a fixed point
//! (at most [`MAX_ITERS`] rounds) → `dce` (unreachable top-level bindings).
//!
//! The simplifier performs β-reduction (term and type), inlining of small
//! non-recursive top-level functions at call sites, inlining of
//! single-occurrence lazy `let`s (occurrence analysis, using the knowledge
//! that a variable bound with multiplicity 1 is used exactly once),
//! case-of-known-constructor / literal, let floating out of application
//! heads, `let` right-hand sides and case scrutinees, constant folding and
//! dead-binding elimination.
//!
//! Every transformation preserves the lazy semantics: a lazy `let` is only
//! moved to its single use (where its thunk would have been forced) and
//! strict evaluation (`let!`, strict fields, `case`) is never removed unless
//! the expression is known to be a total, effect-free value.
//!
//! Every pass is followed by Core Lint on each changed binding. A binding
//! that fails Lint is an internal error in debug builds; release builds
//! keep the binding as it was before the pass.

use crate::expr::*;
use crate::lint;
use crate::ty::*;
use cirka_diagnostics::Diagnostic;
use std::collections::{HashMap, HashSet, VecDeque};

/// Maximum number of simplifier rounds.
pub const MAX_ITERS: usize = 6;
/// Top-level functions at most this big (in expression nodes) are inlined
/// at every call site; bigger ones only when called exactly once.
pub const INLINE_SIZE: usize = 60;

/// Counts of the transformations performed (for tests and measurements).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub iterations: usize,
    pub beta: usize,
    pub inlined: usize,
    pub inlined_once: usize,
    pub unpacked_fields: usize,
    pub known_case: usize,
    pub floated: usize,
    pub folded: usize,
    pub dead: usize,
    pub removed_globals: usize,
    /// Functions split into a worker and a wrapper.
    pub worker_wrapper: usize,
    /// Bindings restored because Lint rejected them (release builds only).
    pub reverted: usize,
}

/// Runs the optimization pipeline.
pub fn optimize(p: &mut Program) -> Result<(), Diagnostic> {
    optimize_with_stats(p).map(|_| ())
}

/// Runs the optimization pipeline and reports what it did.
pub fn optimize_with_stats(p: &mut Program) -> Result<Stats, Diagnostic> {
    let mut st = Stats::default();
    let mut next = p.max_id() + 1;
    for b in &mut p.binds {
        b.expr = freshen(&b.expr, &mut next);
    }
    lint_all(p, "uniquify")?;
    let unpacked = crate::unpack::unpack_strict_fields(p, &mut next);
    if unpacked > 0 {
        st.unpacked_fields += unpacked;
        lint_all(p, "unpack")?;
    }
    simplify_rounds(p, &mut st, &mut next)?;
    // Worker/wrapper splits strict functions over boxed numbers; the
    // simplifier then inlines the wrappers and removes the boxes.
    let split = crate::ww::worker_wrapper(p, &mut next);
    if split > 0 {
        st.worker_wrapper += split;
        lint_all(p, "worker-wrapper")?;
        simplify_rounds(p, &mut st, &mut next)?;
    }
    st.removed_globals = dce_globals(p);
    lint_all(p, "dce")?;
    Ok(st)
}

/// Runs the simplifier to a fixed point (at most [`MAX_ITERS`] rounds).
fn simplify_rounds(p: &mut Program, st: &mut Stats, next: &mut u32) -> Result<(), Diagnostic> {
    for _ in 0..MAX_ITERS {
        st.iterations += 1;
        let inl = inline_candidates(p);
        let kg: HashMap<String, (String, Vec<Type>, Vec<Expr>)> = p
            .binds
            .iter()
            .filter_map(|b| match &b.expr {
                Expr::Con(k, ts, a) if a.iter().all(|e| e.is_atom()) => {
                    Some((b.name.clone(), (k.clone(), ts.clone(), a.clone())))
                }
                _ => None,
            })
            .collect();
        let mut changed = false;
        for i in 0..p.binds.len() {
            let old = p.binds[i].expr.clone();
            let mut occ = HashMap::new();
            occ_analyze(&old, &mut occ);
            let mut s = Simp {
                p,
                inl: &inl,
                occ,
                sub: HashMap::new(),
                env: HashMap::new(),
                known: HashMap::new(),
                known_globals: &kg,
                next: *next,
                changed: false,
                st: Stats::default(),
            };
            let new = s.expr(old.clone());
            *next = s.next;
            if !s.changed {
                continue;
            }
            let local = s.st;
            p.binds[i].expr = new;
            let errs = lint::lint_bind(p, i);
            if errs.is_empty() {
                changed = true;
                add_stats(st, &local);
            } else if cfg!(debug_assertions) {
                return Err(ice("simplify", &p.binds[i].name, &errs, &p.binds[i].expr));
            } else {
                p.binds[i].expr = old;
                st.reverted += 1;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(())
}

fn add_stats(st: &mut Stats, o: &Stats) {
    st.beta += o.beta;
    st.inlined += o.inlined;
    st.inlined_once += o.inlined_once;
    st.known_case += o.known_case;
    st.floated += o.floated;
    st.folded += o.folded;
    st.dead += o.dead;
}

fn ice(pass: &str, name: &str, errs: &[Diagnostic], e: &Expr) -> Diagnostic {
    Diagnostic::ice(format!(
        "Core Lint failed after pass `{pass}` in `{name}`:\n{}\n{}",
        lint::render_errors(errs),
        crate::print::print_expr(e)
    ))
}

/// Lint of the whole program after a pass (debug builds only).
fn lint_all(p: &Program, pass: &str) -> Result<(), Diagnostic> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }
    let errs = lint::lint_program(p);
    if errs.is_empty() {
        Ok(())
    } else {
        Err(Diagnostic::ice(format!(
            "Core Lint failed after pass `{pass}`:\n{}",
            lint::render_errors(&errs)
        )))
    }
}

// ---------------------------------------------------------------------
// Generic helpers

/// Number of expression nodes.
pub fn size(e: &Expr) -> usize {
    let mut n = 0;
    walk(e, &mut |_| n += 1);
    n
}

/// Calls `f` on `e` and every sub-expression.
fn walk(e: &Expr, f: &mut impl FnMut(&Expr)) {
    f(e);
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
        Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => {
            a.iter().for_each(|x| walk(x, f))
        }
        Expr::App(g, a) => {
            walk(g, f);
            walk(a, f);
        }
        Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) | Expr::Lam(_, x) => walk(x, f),
        Expr::Let(Bind::NonRec(_, r), b) | Expr::LetStrict(_, r, b) => {
            walk(r, f);
            walk(b, f);
        }
        Expr::Let(Bind::Rec(bs), b) => {
            bs.iter().for_each(|(_, r)| walk(r, f));
            walk(b, f);
        }
        Expr::Case(s, _, _, alts) => {
            walk(s, f);
            alts.iter().for_each(|a| walk(&a.rhs, f));
        }
    }
}

/// Rebuilds `e` with `f` applied to each direct sub-expression.
fn map_children(e: Expr, f: &mut impl FnMut(Expr) -> Expr) -> Expr {
    let bx = |f: &mut dyn FnMut(Expr) -> Expr, x: Box<Expr>| Box::new(f(*x));
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => e,
        Expr::Con(c, ts, a) => Expr::Con(c, ts, a.into_iter().map(&mut *f).collect()),
        Expr::Prim(o, ts, a) => Expr::Prim(o, ts, a.into_iter().map(&mut *f).collect()),
        Expr::Extern(n, a) => Expr::Extern(n, a.into_iter().map(&mut *f).collect()),
        Expr::App(g, a) => {
            let g = bx(f, g);
            Expr::App(g, bx(f, a))
        }
        Expr::TyApp(x, t) => Expr::TyApp(bx(f, x), t),
        Expr::TyLam(v, k, x) => Expr::TyLam(v, k, bx(f, x)),
        Expr::Cast(x, t) => Expr::Cast(bx(f, x), t),
        Expr::Lam(b, x) => Expr::Lam(b, bx(f, x)),
        Expr::Let(Bind::NonRec(b, r), x) => {
            let r = bx(f, r);
            Expr::Let(Bind::NonRec(b, r), bx(f, x))
        }
        Expr::LetStrict(b, r, x) => {
            let r = bx(f, r);
            Expr::LetStrict(b, r, bx(f, x))
        }
        Expr::Let(Bind::Rec(bs), x) => {
            let bs = bs.into_iter().map(|(b, r)| (b, f(r))).collect();
            Expr::Let(Bind::Rec(bs), bx(f, x))
        }
        Expr::Case(s, b, t, alts) => {
            let s = bx(f, s);
            let alts = alts
                .into_iter()
                .map(|a| Alt {
                    con: a.con,
                    rhs: f(a.rhs),
                })
                .collect();
            Expr::Case(s, b, t, alts)
        }
    }
}

/// Replaces every occurrence of variable `x` by `with`. Binders are unique
/// (after `uniquify`), so no capture can happen.
fn replace_var(e: Expr, x: u32, with: &Expr) -> Expr {
    match e {
        Expr::Var(v) if v.id == x => with.clone(),
        e => map_children(e, &mut |c| replace_var(c, x, with)),
    }
}

/// Occurrences of `x` in `e`: count and whether one is under a lambda.
fn occ_of(e: &Expr, x: u32) -> Occ {
    let mut m = HashMap::new();
    occ_analyze(e, &mut m);
    m.get(&x).copied().unwrap_or_default()
}

fn occurs(e: &Expr, x: u32) -> bool {
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

/// Free term variables of `e`.
fn free_vars(e: &Expr) -> HashSet<u32> {
    fn binders(e: &Expr, out: &mut HashSet<u32>) {
        walk(e, &mut |s| match s {
            Expr::Lam(b, _) | Expr::LetStrict(b, _, _) | Expr::Let(Bind::NonRec(b, _), _) => {
                out.insert(b.id.id);
            }
            Expr::Let(Bind::Rec(bs), _) => out.extend(bs.iter().map(|(b, _)| b.id.id)),
            Expr::Case(_, b, _, alts) => {
                out.insert(b.id.id);
                for a in alts {
                    if let AltCon::Con(_, fs) = &a.con {
                        out.extend(fs.iter().map(|f| f.id.id));
                    }
                }
            }
            _ => {}
        });
    }
    // Binders are unique, so "free" = used and not bound anywhere inside.
    let mut bound = HashSet::new();
    binders(e, &mut bound);
    let mut out = HashSet::new();
    walk(e, &mut |s| {
        if let Expr::Var(v) = s
            && !bound.contains(&v.id)
        {
            out.insert(v.id);
        }
    });
    out
}

/// Type substitution `e[t/v]` in every type annotation of `e`.
fn subst_ty(e: &Expr, v: &TyVar, t: &Type) -> Expr {
    let st = |ty: &Type| ty.subst(v, t);
    let sb = |b: &Binder| Binder::new(b.id.clone(), b.mult, b.ty.subst(v, t));
    let r = |x: &Expr| Box::new(subst_ty(x, v, t));
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) => e.clone(),
        Expr::Panic(m, ty) => Expr::Panic(m.clone(), st(ty)),
        Expr::Con(c, ts, a) => Expr::Con(
            c.clone(),
            ts.iter().map(st).collect(),
            a.iter().map(|x| subst_ty(x, v, t)).collect(),
        ),
        Expr::Prim(o, ts, a) => Expr::Prim(
            *o,
            ts.iter().map(st).collect(),
            a.iter().map(|x| subst_ty(x, v, t)).collect(),
        ),
        Expr::Extern(n, a) => {
            Expr::Extern(n.clone(), a.iter().map(|x| subst_ty(x, v, t)).collect())
        }
        Expr::App(f, a) => Expr::App(r(f), r(a)),
        Expr::TyApp(f, ty) => Expr::TyApp(r(f), st(ty)),
        Expr::Cast(x, ty) => Expr::Cast(r(x), st(ty)),
        Expr::Lam(b, x) => Expr::Lam(sb(b), r(x)),
        Expr::TyLam(w, k, x) => {
            if w == v {
                e.clone()
            } else if t.mentions(w) {
                let w2 = TyVar::fresh(&w.name);
                let x2 = subst_ty(x, w, &Type::Var(w2.clone()));
                Expr::TyLam(w2, k.clone(), Box::new(subst_ty(&x2, v, t)))
            } else {
                Expr::TyLam(w.clone(), k.clone(), r(x))
            }
        }
        Expr::Let(Bind::NonRec(b, x), body) => Expr::Let(Bind::NonRec(sb(b), r(x)), r(body)),
        Expr::LetStrict(b, x, body) => Expr::LetStrict(sb(b), r(x), r(body)),
        Expr::Let(Bind::Rec(bs), body) => Expr::Let(
            Bind::Rec(bs.iter().map(|(b, x)| (sb(b), subst_ty(x, v, t))).collect()),
            r(body),
        ),
        Expr::Case(s, b, res, alts) => Expr::Case(
            r(s),
            sb(b),
            st(res),
            alts.iter()
                .map(|a| Alt {
                    con: match &a.con {
                        AltCon::Con(c, fs) => AltCon::Con(c.clone(), fs.iter().map(sb).collect()),
                        c => c.clone(),
                    },
                    rhs: subst_ty(&a.rhs, v, t),
                })
                .collect(),
        ),
    }
}

/// A copy of `e` with fresh ids for every term binder and fresh type
/// variables for every `Λ`.
pub fn freshen(e: &Expr, next: &mut u32) -> Expr {
    fn nb(b: &Binder, next: &mut u32, m: &mut HashMap<u32, u32>) -> Binder {
        let id = *next;
        *next += 1;
        m.insert(b.id.id, id);
        Binder::new(Id::new(b.id.name.clone(), id), b.mult, b.ty.clone())
    }
    fn go(e: &Expr, next: &mut u32, m: &mut HashMap<u32, u32>) -> Expr {
        match e {
            Expr::Var(v) => match m.get(&v.id) {
                Some(n) => Expr::Var(Id::new(v.name.clone(), *n)),
                None => e.clone(),
            },
            Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => e.clone(),
            Expr::Con(c, ts, a) => Expr::Con(
                c.clone(),
                ts.clone(),
                a.iter().map(|x| go(x, next, m)).collect(),
            ),
            Expr::Prim(o, ts, a) => {
                Expr::Prim(*o, ts.clone(), a.iter().map(|x| go(x, next, m)).collect())
            }
            Expr::Extern(n, a) => {
                Expr::Extern(n.clone(), a.iter().map(|x| go(x, next, m)).collect())
            }
            Expr::App(f, a) => {
                let f = go(f, next, m);
                Expr::App(Box::new(f), Box::new(go(a, next, m)))
            }
            Expr::TyApp(f, t) => Expr::TyApp(Box::new(go(f, next, m)), t.clone()),
            Expr::Cast(x, t) => Expr::Cast(Box::new(go(x, next, m)), t.clone()),
            Expr::TyLam(v, k, x) => {
                let v2 = TyVar::fresh(&v.name);
                let x2 = subst_ty(x, v, &Type::Var(v2.clone()));
                Expr::TyLam(v2, k.clone(), Box::new(go(&x2, next, m)))
            }
            Expr::Lam(b, x) => {
                let b = nb(b, next, m);
                Expr::Lam(b, Box::new(go(x, next, m)))
            }
            Expr::Let(Bind::NonRec(b, r), x) => {
                let r = go(r, next, m);
                let b = nb(b, next, m);
                Expr::Let(Bind::NonRec(b, Box::new(r)), Box::new(go(x, next, m)))
            }
            Expr::LetStrict(b, r, x) => {
                let r = go(r, next, m);
                let b = nb(b, next, m);
                Expr::LetStrict(b, Box::new(r), Box::new(go(x, next, m)))
            }
            Expr::Let(Bind::Rec(bs), x) => {
                let nbs: Vec<Binder> = bs.iter().map(|(b, _)| nb(b, next, m)).collect();
                let rs: Vec<Expr> = bs.iter().map(|(_, r)| go(r, next, m)).collect();
                Expr::Let(
                    Bind::Rec(nbs.into_iter().zip(rs).collect()),
                    Box::new(go(x, next, m)),
                )
            }
            Expr::Case(s, b, t, alts) => {
                let s = go(s, next, m);
                let b = nb(b, next, m);
                let alts = alts
                    .iter()
                    .map(|a| {
                        let con = match &a.con {
                            AltCon::Con(c, fs) => {
                                AltCon::Con(c.clone(), fs.iter().map(|f| nb(f, next, m)).collect())
                            }
                            c => c.clone(),
                        };
                        Alt {
                            con,
                            rhs: go(&a.rhs, next, m),
                        }
                    })
                    .collect();
                Expr::Case(Box::new(s), b, t.clone(), alts)
            }
        }
    }
    go(e, next, &mut HashMap::new())
}

// ---------------------------------------------------------------------
// Occurrence analysis

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Occ {
    count: u32,
    /// Some occurrence is inside a lambda (or a recursive binding), where
    /// it may be evaluated many times.
    in_lam: bool,
}

/// Occurrence counts of every variable (ids are unique within a binding).
/// An occurrence is "under a lambda" when a lambda (or recursive binding)
/// lies between it and its binder.
fn occ_analyze(e: &Expr, m: &mut HashMap<u32, Occ>) {
    fn binders(e: &Expr) -> Vec<u32> {
        match e {
            Expr::Lam(b, _) | Expr::LetStrict(b, _, _) | Expr::Let(Bind::NonRec(b, _), _) => {
                vec![b.id.id]
            }
            Expr::Let(Bind::Rec(bs), _) => bs.iter().map(|(b, _)| b.id.id).collect(),
            Expr::Case(_, b, _, alts) => {
                let mut v = vec![b.id.id];
                for a in alts {
                    if let AltCon::Con(_, fs) = &a.con {
                        v.extend(fs.iter().map(|f| f.id.id));
                    }
                }
                v
            }
            _ => vec![],
        }
    }
    fn go(e: &Expr, depth: u32, m: &mut HashMap<u32, Occ>, bd: &mut HashMap<u32, u32>) {
        for b in binders(e) {
            bd.insert(b, depth);
            m.entry(b).or_default();
        }
        match e {
            Expr::Var(v) => {
                let under = depth > bd.get(&v.id).copied().unwrap_or(0);
                let o = m.entry(v.id).or_default();
                o.count = o.count.saturating_add(1);
                o.in_lam |= under;
            }
            Expr::Lam(b, x) => {
                // The parameter is bound inside the lambda.
                bd.insert(b.id.id, depth + 1);
                go(x, depth + 1, m, bd);
            }
            Expr::Let(Bind::Rec(bs), x) => {
                bs.iter().for_each(|(_, r)| go(r, depth + 1, m, bd));
                go(x, depth, m, bd);
            }
            Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
            Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => {
                a.iter().for_each(|x| go(x, depth, m, bd))
            }
            Expr::App(f, a) => {
                go(f, depth, m, bd);
                go(a, depth, m, bd);
            }
            Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) => go(x, depth, m, bd),
            Expr::Let(Bind::NonRec(_, r), x) | Expr::LetStrict(_, r, x) => {
                go(r, depth, m, bd);
                go(x, depth, m, bd);
            }
            Expr::Case(s, _, _, alts) => {
                go(s, depth, m, bd);
                alts.iter().for_each(|a| go(&a.rhs, depth, m, bd));
            }
        }
    }
    go(e, 0, m, &mut HashMap::new());
}

// ---------------------------------------------------------------------
// Inlining candidates and global DCE

fn globals_in(e: &Expr, out: &mut Vec<String>) {
    walk(e, &mut |s| {
        if let Expr::Global(g) = s {
            out.push(g.clone());
        }
    });
}

/// Top-level functions that may be inlined at call sites: non-recursive
/// lambdas (possibly under `Λ`s) that are small or called exactly once.
fn inline_candidates(p: &Program) -> HashMap<String, Expr> {
    let refs: HashMap<&str, Vec<String>> = p
        .binds
        .iter()
        .map(|b| {
            let mut v = Vec::new();
            globals_in(&b.expr, &mut v);
            (b.name.as_str(), v)
        })
        .collect();
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for v in refs.values() {
        for g in v {
            *uses.entry(g.as_str()).or_default() += 1;
        }
    }
    let recursive = |name: &str| {
        let mut seen = HashSet::new();
        let mut stack: Vec<&str> = refs
            .get(name)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();
        while let Some(g) = stack.pop() {
            if g == name {
                return true;
            }
            if seen.insert(g)
                && let Some(v) = refs.get(g)
            {
                stack.extend(v.iter().map(|s| s.as_str()));
            }
        }
        false
    };
    let mut out = HashMap::new();
    for b in &p.binds {
        let mut body = &b.expr;
        while let Expr::TyLam(_, _, x) = body {
            body = x;
        }
        // Aliases (`(+) = primIntAdd`) are inlined like small functions.
        let alias = {
            let mut a = body;
            while let Expr::TyApp(x, _) = a {
                a = x;
            }
            matches!(a, Expr::Global(g) if *g != b.name)
        };
        if b.name == "main" || !(matches!(body, Expr::Lam(..)) || alias) {
            continue;
        }
        let small = size(&b.expr) <= INLINE_SIZE;
        let once = uses.get(b.name.as_str()) == Some(&1);
        // A wrapper of the worker/wrapper split only calls its worker: it
        // is always inlined, even though the worker may call it back.
        let wrapper = p.bind(&format!("{}$w", b.name)).is_some();
        if wrapper || ((small || once) && !recursive(&b.name)) {
            out.insert(b.name.clone(), b.expr.clone());
        }
    }
    out
}

/// Removes top-level bindings unreachable from `main` (when there is one).
fn dce_globals(p: &mut Program) -> usize {
    if p.bind("main").is_none() {
        return 0;
    }
    let mut live: HashSet<String> = HashSet::new();
    let mut stack = vec!["main".to_string()];
    while let Some(g) = stack.pop() {
        if !live.insert(g.clone()) {
            continue;
        }
        if let Some(b) = p.bind(&g) {
            globals_in(&b.expr, &mut stack);
        }
    }
    let before = p.binds.len();
    p.binds.retain(|b| live.contains(&b.name));
    before - p.binds.len()
}

// ---------------------------------------------------------------------
// The simplifier

struct Simp<'a> {
    p: &'a Program,
    inl: &'a HashMap<String, Expr>,
    occ: HashMap<u32, Occ>,
    /// Pending substitutions: atoms (copied at every occurrence) or
    /// single-occurrence expressions (moved to their occurrence).
    sub: HashMap<u32, Expr>,
    /// Multiplicity and unboxedness of every binder seen so far.
    env: HashMap<u32, (Mult, bool)>,
    /// Variables known to be bound to a constructor applied to atoms.
    known: HashMap<u32, (String, Vec<Type>, Vec<Expr>)>,
    /// Top-level bindings that are constructors applied to atoms
    /// (e.g. type-class dictionaries).
    known_globals: &'a HashMap<String, (String, Vec<Type>, Vec<Expr>)>,
    next: u32,
    changed: bool,
    st: Stats,
}

/// The rest of a binding sequence being built by case-of-known-constructor.
enum K {
    Expr(Expr),
    Fields(VecDeque<(Binder, Expr, bool)>, Option<(Binder, Expr)>, Expr),
}

const MANY: Occ = Occ {
    count: u32::MAX,
    in_lam: true,
};

impl Simp<'_> {
    fn hit(&mut self) {
        self.changed = true;
    }

    fn bind(&mut self, b: &Binder) {
        self.env.insert(b.id.id, (b.mult, b.ty.is_unboxed()));
    }

    fn occ(&self, x: u32) -> Occ {
        self.occ.get(&x).copied().unwrap_or(MANY)
    }

    fn uses_linear(&self, e: &Expr) -> bool {
        free_vars(e)
            .iter()
            .any(|v| self.env.get(v).is_some_and(|(m, _)| *m == Mult::One))
    }

    fn is_unboxed_var(&self, e: &Expr) -> bool {
        matches!(e, Expr::Var(v) if self.env.get(&v.id).is_some_and(|(_, u)| *u))
    }

    /// `x` is substituted by atom `a`: its occurrences become `a`'s, and
    /// the occurrence of `a` that bound `x` disappears.
    fn transfer_occ(&mut self, x: u32, a: &Expr) {
        self.transfer_occ_by(x, a, 1);
    }

    /// Like `transfer_occ`, but `removed` occurrences of `a` disappear (0
    /// when `a` stays where it was, e.g. the field of a known constructor
    /// that remains bound).
    fn transfer_occ_by(&mut self, x: u32, a: &Expr, removed: u32) {
        let mut a = a;
        while let Expr::TyApp(e, _) | Expr::Cast(e, _) = a {
            a = e;
        }
        if let Expr::Var(y) = a {
            let ox = self.occ(x);
            let oy = self.occ.entry(y.id).or_default();
            oy.count = oy.count.saturating_add(ox.count).saturating_sub(removed);
            oy.in_lam |= ox.in_lam;
        }
    }

    fn remember(&mut self, x: u32, r: &Expr) {
        if let Expr::Con(k, ts, a) = r
            && a.iter().all(|e| e.is_atom())
        {
            self.known.insert(x, (k.clone(), ts.clone(), a.clone()));
        }
    }

    fn expr(&mut self, e: Expr) -> Expr {
        match e {
            Expr::Var(x) => match self.sub.get(&x.id) {
                Some(r) if r.is_atom() => r.clone(),
                Some(_) => self.sub.remove(&x.id).unwrap(),
                None => Expr::Var(x),
            },
            Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => e,
            Expr::Con(c, ts, a) => Expr::Con(c, ts, a.into_iter().map(|x| self.expr(x)).collect()),
            Expr::Extern(n, a) => Expr::Extern(n, a.into_iter().map(|x| self.expr(x)).collect()),
            Expr::Prim(op, ts, a) => {
                let a: Vec<Expr> = a.into_iter().map(|x| self.expr(x)).collect();
                match fold(op, &a) {
                    Some(l) => {
                        self.hit();
                        self.st.folded += 1;
                        Expr::Lit(l)
                    }
                    None => Expr::Prim(op, ts, a),
                }
            }
            Expr::App(f, a) => {
                let f = self.callee(*f);
                let a = self.expr(*a);
                self.apply(f, a)
            }
            Expr::TyApp(f, t) => {
                let f = self.expr(*f);
                self.ty_apply(f, t)
            }
            Expr::Lam(b, x) => {
                self.bind(&b);
                Expr::Lam(b, Box::new(self.expr(*x)))
            }
            Expr::TyLam(v, k, x) => Expr::TyLam(v, k, Box::new(self.expr(*x))),
            Expr::Cast(x, t) => Expr::Cast(Box::new(self.expr(*x)), t),
            Expr::Let(Bind::NonRec(b, r), body) => {
                let o = self.occ(b.id.id);
                if o.count == 0 && b.mult != Mult::One {
                    self.hit();
                    self.st.dead += 1;
                    return self.expr(*body);
                }
                let r = self.expr(*r);
                self.let_lazy(b, r, K::Expr(*body))
            }
            Expr::Let(Bind::Rec(bs), body) => {
                for (b, _) in &bs {
                    self.bind(b);
                }
                let bs: Vec<(Binder, Expr)> =
                    bs.into_iter().map(|(b, r)| (b, self.expr(r))).collect();
                let body = self.expr(*body);
                if bs.iter().all(|(b, _)| !occurs(&body, b.id.id)) {
                    self.hit();
                    self.st.dead += 1;
                    return body;
                }
                Expr::Let(Bind::Rec(bs), Box::new(body))
            }
            Expr::LetStrict(b, r, body) => {
                let r = self.expr(*r);
                self.let_strict(b, r, K::Expr(*body))
            }
            Expr::Case(s, b, res, alts) => {
                let s = self.expr(*s);
                self.case(s, b, res, alts)
            }
        }
    }

    /// Simplifies the head of an application, inlining a top-level
    /// function there.
    fn callee(&mut self, f: Expr) -> Expr {
        match f {
            Expr::Global(g) => match self.inl.get(&g) {
                Some(body) => {
                    self.hit();
                    self.st.inlined += 1;
                    let copy = freshen(body, &mut self.next);
                    self.expr(copy)
                }
                None => Expr::Global(g),
            },
            Expr::TyApp(h, t) => {
                let h = self.callee(*h);
                self.ty_apply(h, t)
            }
            f => self.expr(f),
        }
    }

    fn run(&mut self, k: K) -> Expr {
        match k {
            K::Expr(e) => self.expr(e),
            K::Fields(mut items, cb, rhs) => match items.pop_front() {
                None => match cb {
                    Some((b, con)) => {
                        let con = self.expr(con);
                        self.let_lazy(b, con, K::Expr(rhs))
                    }
                    None => self.expr(rhs),
                },
                Some((f, a, strict)) => {
                    let rest = K::Fields(items, cb, rhs);
                    if strict {
                        self.let_strict(f, a, rest)
                    } else {
                        self.let_lazy(f, a, rest)
                    }
                }
            },
        }
    }

    /// `f a` with both simplified.
    fn apply(&mut self, f: Expr, a: Expr) -> Expr {
        match f {
            Expr::Lam(b, body) => self.beta(b, *body, a),
            Expr::Let(bind, body) => {
                self.hit();
                self.st.floated += 1;
                Expr::Let(bind, Box::new(self.apply(*body, a)))
            }
            Expr::LetStrict(b, r, body) => {
                self.hit();
                self.st.floated += 1;
                Expr::LetStrict(b, r, Box::new(self.apply(*body, a)))
            }
            f => Expr::App(Box::new(f), Box::new(a)),
        }
    }

    /// `(λb. body) a` (both simplified).
    fn beta(&mut self, b: Binder, body: Expr, a: Expr) -> Expr {
        let x = b.id.id;
        let done = |s: &mut Self, e: Expr| {
            s.hit();
            s.st.beta += 1;
            e
        };
        if a.is_atom() {
            return done(self, replace_var(body, x, &a));
        }
        if b.ty.is_unboxed() {
            // Unboxed arguments are evaluated before the call.
            return done(self, Expr::LetStrict(b, Box::new(a), Box::new(body)));
        }
        if b.mult == Mult::Many || a.is_value() {
            return done(
                self,
                Expr::Let(Bind::NonRec(b, Box::new(a)), Box::new(body)),
            );
        }
        // A linear (or erased) argument that cannot become a lazy `let`:
        // substitute it at its single occurrence.
        let o = occ_of(&body, x);
        if o.count == 1 && !o.in_lam {
            return done(self, replace_var(body, x, &a));
        }
        if o.count == 0 && b.mult == Mult::Zero {
            return done(self, body);
        }
        Expr::App(Box::new(Expr::Lam(b, Box::new(body))), Box::new(a))
    }

    fn ty_apply(&mut self, f: Expr, t: Type) -> Expr {
        match f {
            Expr::TyLam(v, _, body) => {
                self.hit();
                self.st.beta += 1;
                subst_ty(&body, &v, &t)
            }
            Expr::Let(bind, body) => {
                self.hit();
                self.st.floated += 1;
                Expr::Let(bind, Box::new(self.ty_apply(*body, t)))
            }
            Expr::LetStrict(b, r, body) => {
                self.hit();
                self.st.floated += 1;
                Expr::LetStrict(b, r, Box::new(self.ty_apply(*body, t)))
            }
            f => Expr::TyApp(Box::new(f), t),
        }
    }

    /// Lazy `let b = r in k` with `r` simplified.
    fn let_lazy(&mut self, b: Binder, r: Expr, k: K) -> Expr {
        if let Expr::Let(inner, r2) = r {
            // let b = (let y = e in r2) in k  ⇒  let y = e in let b = r2 in k
            self.hit();
            self.st.floated += 1;
            return Expr::Let(inner, Box::new(self.let_lazy(b, *r2, k)));
        }
        if let Expr::Con(c, ts, args) = &r
            && args.iter().any(|a| !a.is_atom())
            && args.iter().all(|a| a.is_atom() || self.total(a))
            && let Some(fts) = self.strict_field_types(c, ts)
        {
            // let b = K e (e total)  ⇒  let! u = e in let b = K u: no thunk
            // for a constructor whose strict fields are cheap to compute.
            self.hit();
            self.st.floated += 1;
            let Expr::Con(c, ts, args) = r else {
                unreachable!()
            };
            let mut binds = Vec::new();
            let mut atoms = Vec::new();
            for (a, ft) in args.into_iter().zip(fts) {
                if a.is_atom() {
                    atoms.push(a);
                } else {
                    let u = Binder::new(Id::new("u", self.next), Mult::Many, ft);
                    self.next += 1;
                    self.bind(&u);
                    atoms.push(Expr::Var(u.id.clone()));
                    binds.push((u, a));
                }
            }
            let mut body = self.let_lazy(b, Expr::Con(c, ts, atoms), k);
            for (u, a) in binds.into_iter().rev() {
                body = Expr::LetStrict(u, Box::new(a), Box::new(body));
            }
            return body;
        }
        self.bind(&b);
        let x = b.id.id;
        if r.is_atom() {
            self.hit();
            self.st.inlined_once += 1;
            self.transfer_occ(x, &r);
            self.sub.insert(x, r);
            return self.run(k);
        }
        let o = self.occ(x);
        // Constructor values stay bound: that costs the same allocation and
        // keeps `case x` (case-of-known-constructor) and atomic fields.
        if o.count == 1 && !o.in_lam && !matches!(r, Expr::Con(..)) {
            // Moved to its only use, where the thunk would be forced.
            self.hit();
            self.st.inlined_once += 1;
            self.sub.insert(x, r);
            return self.run(k);
        }
        self.remember(x, &r);
        let body = self.run(k);
        Expr::Let(Bind::NonRec(b, Box::new(r)), Box::new(body))
    }

    /// `let! b = r in k` with `r` simplified.
    fn let_strict(&mut self, b: Binder, r: Expr, k: K) -> Expr {
        match r {
            Expr::Let(inner, r2) => {
                self.hit();
                self.st.floated += 1;
                return Expr::Let(inner, Box::new(self.let_strict(b, *r2, k)));
            }
            Expr::LetStrict(b2, r2, rest) => {
                self.hit();
                self.st.floated += 1;
                return Expr::LetStrict(b2, r2, Box::new(self.let_strict(b, *rest, k)));
            }
            _ => {}
        }
        if let Expr::Con(c, ts, args) = &r
            && args.iter().any(|a| !a.is_atom())
            && let Some(fts) = self.strict_field_types(c, ts)
        {
            // let! b = K e  ⇒  let! u = e in let! b = K u  (strict fields
            // are evaluated before construction anyway), so that `b` is a
            // known constructor.
            self.hit();
            self.st.floated += 1;
            let Expr::Con(c, ts, args) = r else {
                unreachable!()
            };
            let mut binds = Vec::new();
            let mut atoms = Vec::new();
            for (a, ft) in args.into_iter().zip(fts) {
                if a.is_atom() {
                    atoms.push(a);
                } else {
                    let u = Binder::new(Id::new("u", self.next), Mult::Many, ft);
                    self.next += 1;
                    self.bind(&u);
                    atoms.push(Expr::Var(u.id.clone()));
                    binds.push((u, a));
                }
            }
            let mut body = self.let_strict(b, Expr::Con(c, ts, atoms), k);
            for (u, a) in binds.into_iter().rev() {
                body = Expr::LetStrict(u, Box::new(a), Box::new(body));
            }
            return body;
        }
        self.bind(&b);
        let x = b.id.id;
        if matches!(r, Expr::Lit(_)) || self.is_unboxed_var(&r) {
            // Already evaluated: plain substitution.
            self.hit();
            self.st.inlined_once += 1;
            self.transfer_occ(x, &r);
            self.sub.insert(x, r);
            return self.run(k);
        }
        self.remember(x, &r);
        let body = self.run(k);
        if self.occ(x).count == 0 && b.mult != Mult::One && self.total(&r) && !self.uses_linear(&r)
        {
            self.hit();
            self.st.dead += 1;
            return body;
        }
        Expr::LetStrict(b, Box::new(r), Box::new(body))
    }

    /// Whether `case (case .. of alts2) as b of alts` may become a case in
    /// each alternative of the inner one: no linear binders, and either a
    /// single inner alternative or inner results whose shape is known
    /// (so the copies of `alts` reduce), unless `alts` is small.
    fn case_of_case_ok(&self, b: &Binder, alts: &[Alt], alts2: &[Alt]) -> bool {
        let linear = |a: &Alt| match &a.con {
            AltCon::Con(_, fs) => fs.iter().any(|f| f.mult != Mult::Many),
            _ => false,
        };
        if b.mult != Mult::Many || alts.iter().any(linear) || alts2.iter().any(linear) {
            return false;
        }
        if alts2.len() == 1 {
            return true;
        }
        // A pending single-use substitution would be consumed by the first
        // copy of `alts`; keep those cases as they are.
        let pending = alts.iter().any(|a| {
            free_vars(&a.rhs)
                .iter()
                .any(|x| self.sub.get(x).is_some_and(|r| !r.is_atom()))
        });
        if pending {
            return false;
        }
        let known = alts2
            .iter()
            .all(|a| matches!(a.rhs, Expr::Con(..) | Expr::Lit(_) | Expr::Panic(..)));
        let small = alts.iter().map(|a| size(&a.rhs)).sum::<usize>() <= INLINE_SIZE;
        known || small
    }

    /// Field types of constructor `c` at type arguments `ts` when all its
    /// fields are strict and not linear.
    fn strict_field_types(&self, c: &str, ts: &[Type]) -> Option<Vec<Type>> {
        let (d, i) = self.p.ctor(c)?;
        let k = &d.ctors[i];
        if d.linear || !k.strict.iter().all(|s| *s) || d.params.len() != ts.len() {
            return None;
        }
        let s: Vec<(TyVar, Type)> = d
            .params
            .iter()
            .map(|p| p.0.clone())
            .zip(ts.iter().cloned())
            .collect();
        Some(k.fields.iter().map(|f| f.subst_all(&s)).collect())
    }

    /// Evaluating `e` terminates without effects (so an unused `let!` of it
    /// can be removed).
    fn total(&self, e: &Expr) -> bool {
        match e {
            Expr::Lit(_) | Expr::Lam(..) => true,
            Expr::TyLam(_, _, x) => self.total(x),
            Expr::Var(_) => self.is_unboxed_var(e),
            Expr::Prim(op, _, a) => !op.effectful() && a.iter().all(|x| self.total(x)),
            Expr::Con(c, _, a) => {
                let strict = self
                    .p
                    .ctor(c)
                    .map(|(d, i)| d.ctors[i].strict.clone())
                    .unwrap_or_default();
                a.iter().enumerate().all(|(i, x)| {
                    self.total(x) || (!strict.get(i).copied().unwrap_or(true) && x.is_atom())
                })
            }
            _ => false,
        }
    }

    fn case(&mut self, s: Expr, b: Binder, res: Type, alts: Vec<Alt>) -> Expr {
        match s {
            Expr::Let(inner, s2) => {
                self.hit();
                self.st.floated += 1;
                Expr::Let(inner, Box::new(self.case(*s2, b, res, alts)))
            }
            Expr::LetStrict(b2, r2, s2) => {
                self.hit();
                self.st.floated += 1;
                Expr::LetStrict(b2, r2, Box::new(self.case(*s2, b, res, alts)))
            }
            Expr::Case(s2, b2, _, alts2) if self.case_of_case_ok(&b, &alts, &alts2) => {
                // case (case s2 of p -> e) of alts  ⇒  case s2 of p -> case e of alts
                self.hit();
                self.st.known_case += 1;
                let n = alts2.len();
                let outer = Expr::Case(Box::new(Expr::Lit(Lit::Unit)), b, res.clone(), alts);
                let alts2 = alts2
                    .into_iter()
                    .map(|a| {
                        let Expr::Case(_, b, r, alts) = (if n > 1 {
                            freshen(&outer, &mut self.next)
                        } else {
                            outer.clone()
                        }) else {
                            unreachable!()
                        };
                        let rhs = self.case(a.rhs, b, r, alts);
                        Alt { con: a.con, rhs }
                    })
                    .collect();
                Expr::Case(s2, b2, res, alts2)
            }
            Expr::Lit(Lit::Int(n)) => {
                let pick = alts
                    .iter()
                    .position(|a| a.con == AltCon::Int(n))
                    .or_else(|| alts.iter().position(|a| a.con == AltCon::Default));
                if let Some(i) = pick {
                    self.hit();
                    self.st.known_case += 1;
                    self.bind(&b);
                    self.sub.insert(b.id.id, Expr::Lit(Lit::Int(n)));
                    let rhs = alts.into_iter().nth(i).unwrap().rhs;
                    return self.expr(rhs);
                }
                self.case_default(Expr::Lit(Lit::Int(n)), b, res, alts)
            }
            Expr::Con(..) => self.known_con(s, b, res, alts),
            Expr::Var(x) if self.known.contains_key(&x.id) => {
                let con = self.known[&x.id].clone();
                let lin = self.env.get(&x.id).is_none_or(|(m, _)| *m != Mult::Many);
                self.known_scrut(Expr::Var(x), lin, con, b, res, alts)
            }
            Expr::Global(g) if self.known_globals.contains_key(&g) => {
                let con = self.known_globals[&g].clone();
                self.known_scrut(Expr::Global(g), false, con, b, res, alts)
            }
            s => self.case_default(s, b, res, alts),
        }
    }

    /// `case x as b of alts` where `x` (a variable or a global) is bound to
    /// `K ts args` (atoms): the case binder becomes `x` and the fields
    /// become the arguments.
    #[allow(clippy::too_many_arguments)]
    fn known_scrut(
        &mut self,
        x: Expr,
        x_linear: bool,
        (k, _, args): (String, Vec<Type>, Vec<Expr>),
        b: Binder,
        res: Type,
        alts: Vec<Alt>,
    ) -> Expr {
        let pick = alts
            .iter()
            .position(|a| matches!(&a.con, AltCon::Con(c, _) if *c == k))
            .or_else(|| alts.iter().position(|a| a.con == AltCon::Default));
        let (Some(i), false) = (pick, x_linear || args.iter().any(|a| self.uses_linear(a))) else {
            return self.case_default(x, b, res, alts);
        };
        self.hit();
        self.st.known_case += 1;
        let alt = alts.into_iter().nth(i).unwrap();
        self.bind(&b);
        let xv = x;
        self.transfer_occ(b.id.id, &xv);
        self.sub.insert(b.id.id, xv);
        if let AltCon::Con(_, fs) = &alt.con {
            for (f, a) in fs.iter().zip(args) {
                self.bind(f);
                // The constructor keeps its fields: nothing is removed.
                self.transfer_occ_by(f.id.id, &a, 0);
                self.sub.insert(f.id.id, a);
            }
        }
        self.expr(alt.rhs)
    }

    fn case_default(&mut self, s: Expr, b: Binder, res: Type, alts: Vec<Alt>) -> Expr {
        self.bind(&b);
        let scrut_var = match &s {
            Expr::Var(v) => Some(v.id),
            _ => None,
        };
        let targs = match &b.ty {
            Type::Con(_, ts) => ts.clone(),
            _ => Vec::new(),
        };
        let alts = alts
            .into_iter()
            .map(|a| {
                if let AltCon::Con(c, fs) = &a.con {
                    for f in fs {
                        self.bind(f);
                    }
                    // Inside the alternative the scrutinee is known.
                    let con = (
                        c.clone(),
                        targs.clone(),
                        fs.iter().map(|f| Expr::Var(f.id.clone())).collect(),
                    );
                    self.known.insert(b.id.id, con.clone());
                    if let Some(v) = scrut_var {
                        self.known.insert(v, con);
                    }
                }
                let rhs = self.expr(a.rhs);
                self.known.remove(&b.id.id);
                if let Some(v) = scrut_var {
                    self.known.remove(&v);
                }
                Alt { con: a.con, rhs }
            })
            .collect();
        Expr::Case(Box::new(s), b, res, alts)
    }

    /// `case K ts args as b of alts`.
    fn known_con(&mut self, s: Expr, b: Binder, res: Type, alts: Vec<Alt>) -> Expr {
        let Expr::Con(k, ts, args) = s else {
            unreachable!()
        };
        let strict = match self.p.ctor(&k) {
            Some((d, i)) => d.ctors[i].strict.clone(),
            None => return self.case_default(Expr::Con(k, ts, args), b, res, alts),
        };
        let pick = alts
            .iter()
            .position(|a| matches!(&a.con, AltCon::Con(c, _) if *c == k))
            .or_else(|| alts.iter().position(|a| a.con == AltCon::Default));
        let Some(i) = pick else {
            return self.case_default(Expr::Con(k, ts, args), b, res, alts);
        };
        let b_used = self.occ(b.id.id).count > 0;
        let ok = match &alts[i].con {
            // case K as b of _ -> rhs  ≡  let! b = K in rhs
            AltCon::Default => b.mult != Mult::One,
            AltCon::Con(_, fs) => {
                !(b_used && (b.mult == Mult::One || fs.iter().any(|f| f.mult == Mult::One)))
                    && fs.iter().zip(&args).zip(&strict).all(|((f, a), st)| {
                        if f.mult == Mult::One {
                            *st || a.is_value() || !self.uses_linear(a)
                        } else {
                            !self.uses_linear(a)
                        }
                    })
            }
            AltCon::Int(_) => false,
        };
        if !ok {
            return self.case_default(Expr::Con(k, ts, args), b, res, alts);
        }
        self.hit();
        self.st.known_case += 1;
        let alt = alts.into_iter().nth(i).unwrap();
        match alt.con {
            AltCon::Con(_, fs) => {
                let cb = if b_used {
                    for f in &fs {
                        if let Some(o) = self.occ.get_mut(&f.id.id) {
                            o.count = o.count.saturating_add(1);
                        }
                    }
                    let vars = fs.iter().map(|f| Expr::Var(f.id.clone())).collect();
                    Some((b, Expr::Con(k, ts, vars)))
                } else {
                    None
                };
                let items = fs
                    .into_iter()
                    .zip(args)
                    .zip(strict)
                    .map(|((f, a), st)| (f, a, st))
                    .collect();
                self.run(K::Fields(items, cb, alt.rhs))
            }
            _ => self.let_strict(b, Expr::Con(k, ts, args), K::Expr(alt.rhs)),
        }
    }
}

/// Constant folding of primitive operations on literals.
fn fold(op: PrimOp, a: &[Expr]) -> Option<Lit> {
    use PrimOp::*;
    let int = |i: usize| match a.get(i) {
        Some(Expr::Lit(Lit::Int(n))) => Some(*n),
        _ => None,
    };
    let dbl = |i: usize| match a.get(i) {
        Some(Expr::Lit(Lit::Double(n))) => Some(*n),
        _ => None,
    };
    let b = |x: bool| Some(Lit::Int(x as i64));
    let f = |x: f64| x.is_finite().then_some(Lit::Double(x));
    match op {
        Add => Some(Lit::Int(int(0)?.wrapping_add(int(1)?))),
        Sub => Some(Lit::Int(int(0)?.wrapping_sub(int(1)?))),
        Mul => Some(Lit::Int(int(0)?.wrapping_mul(int(1)?))),
        Neg => Some(Lit::Int(int(0)?.wrapping_neg())),
        And => Some(Lit::Int(int(0)? & int(1)?)),
        Or => Some(Lit::Int(int(0)? | int(1)?)),
        Xor => Some(Lit::Int(int(0)? ^ int(1)?)),
        Eq => b(int(0)? == int(1)?),
        Ne => b(int(0)? != int(1)?),
        Lt => b(int(0)? < int(1)?),
        Le => b(int(0)? <= int(1)?),
        Gt => b(int(0)? > int(1)?),
        Ge => b(int(0)? >= int(1)?),
        FAdd => f(dbl(0)? + dbl(1)?),
        FSub => f(dbl(0)? - dbl(1)?),
        FMul => f(dbl(0)? * dbl(1)?),
        FDiv => f(dbl(0)? / dbl(1)?),
        FNeg => f(-dbl(0)?),
        FEq => b(dbl(0)? == dbl(1)?),
        FLt => b(dbl(0)? < dbl(1)?),
        FLe => b(dbl(0)? <= dbl(1)?),
        IntToDouble => f(int(0)? as f64),
        _ => None,
    }
}
