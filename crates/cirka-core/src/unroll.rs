//! Unrolling of loops on literal arguments (FEAT-0094).
//!
//! A tail-recursive function (or a group of them) whose `Int#` parameters
//! start from literals, move by literal steps and decide where the
//! recursion goes runs the same way every time. It is copied for each
//! tuple of literals its tail calls reach from a call site, with the
//! literals substituted and the branches they decide taken:
//!
//! ```text
//! pairs n i j = if i >= 5 then moves n 0
//!               else if j >= 5 then pairs n (i + 1) (i + 2)
//!               else … i * 7 … pairs n i (j + 1)
//! … pairs n 0 1 …
//! ==>
//! pairs$kx_0_1 n = … 0 * 7 … ((λn. … 0 * 7 … ) n)       -- pairs n 0 2 inlined
//! … pairs$kx_0_1 n …
//! ```
//!
//! A copy that only one tail call reaches is inlined there; the others
//! (the call sites' and those reached more than once) stay functions.
//! A fixed loop thus becomes straight-line code with literal indices, as
//! LLVM fully unrolls a Rust loop with constant bounds (bench/n-body).
//!
//! A call site is specialized only when the copies it reaches are at most
//! [`MAX_COPIES`] and [`MAX_NODES`] in size (and [`MAX_TOTAL_NODES`] for
//! the whole program): a loop bounded by a variable
//! reaches new literals without end and is left alone. Only tail calls are
//! followed, so a recursion that is not a loop (`fib 30`) is not unrolled,
//! and a copy keeps at least one parameter (no constant is made of a loop).

use crate::expr::*;
use crate::opt::{fold, freshen, map_children, replace_var, size, walk};
use crate::ty::*;
use crate::ww::split_fun;
use std::collections::{HashMap, HashSet};

/// At most this many copies for one call site.
pub const MAX_COPIES: usize = 64;
/// At most this many Core nodes in the copies for one call site (about a
/// quarter as many instructions).
pub const MAX_NODES: usize = 8_000;
/// At most this many Core nodes in all the copies of a program.
pub const MAX_TOTAL_NODES: usize = 40_000;
/// The literals of a call site among which subsets are tried (the others
/// stay literals).
const MAX_FORGET: usize = 4;

/// A recursive monomorphic function with `Int#` parameters.
struct Fun {
    ty: Type,
    params: Vec<Binder>,
    body: Expr,
}

/// A copy: the function and the literal of each parameter that has one.
type Key = (String, Vec<Option<i64>>);

struct Unroller {
    funs: HashMap<String, Fun>,
    /// The bodies of the copies, literals substituted and folded.
    bodies: HashMap<Key, Expr>,
}

/// Copies the loops called with literal arguments; returns the number of
/// copies made (inlined ones included).
pub fn unroll_literal_loops(p: &mut Program, next: &mut u32) -> usize {
    let mut u = Unroller {
        funs: candidates(p),
        bodies: HashMap::new(),
    };
    if u.funs.is_empty() {
        return 0;
    }
    // The call sites, each to be specialized to some key: every call
    // outside the candidates, and the calls inside them that are not tail
    // calls (the entries of inner loops; their tail calls are followed
    // from the copies instead).
    let mut roots: Vec<Key> = Vec::new();
    for b in &p.binds {
        match u.funs.get(&b.name) {
            Some(f) => u.roots_in(&f.body, false, &mut roots),
            None => u.roots_in(&b.expr, true, &mut roots),
        }
    }
    roots.sort();
    roots.dedup();
    let mut accepted: HashSet<Key> = HashSet::new();
    let mut root_map: HashMap<Key, Key> = HashMap::new();
    let mut budget = MAX_TOTAL_NODES;
    for r in roots {
        if accepted.contains(&r) {
            root_map.insert(r.clone(), r);
        } else if let Some((g, ks)) = u.closure_any(&r) {
            let nodes: usize = ks
                .iter()
                .filter(|k| !accepted.contains(*k))
                .map(|k| size(&u.bodies[k]))
                .sum();
            if nodes > budget {
                continue;
            }
            budget -= nodes;
            accepted.extend(ks);
            root_map.insert(r, g);
        }
    }
    if accepted.is_empty() {
        return 0;
    }
    // A copy reached by one tail call (and no call site) is inlined there,
    // when that call's arguments can be bound to its parameters.
    let mut refs: HashMap<&Key, usize> = HashMap::new();
    let mut unbindable: HashSet<&Key> = HashSet::new();
    for k in &accepted {
        for (t, call) in tail_calls(&u, &u.bodies[k]) {
            if let Some(k2) = accepted.get(&t) {
                *refs.entry(k2).or_default() += 1;
                if !u.bindable(k2, call) {
                    unbindable.insert(k2);
                }
            }
        }
    }
    let entries: HashSet<&Key> = root_map.values().collect();
    let kept: HashSet<Key> = accepted
        .iter()
        .filter(|k| {
            entries.contains(k) || unbindable.contains(k) || refs.get(k).copied().unwrap_or(0) != 1
        })
        .cloned()
        .collect();
    let names: HashMap<Key, String> = kept.iter().map(|k| (k.clone(), copy_name(&u, k))).collect();
    let mut keys: Vec<&Key> = kept.iter().collect();
    keys.sort();
    let mut new = Vec::new();
    for k in keys {
        let expr = u.build(k, &accepted, &names, next);
        let expr = u.rewrite_calls(expr, &root_map, &names);
        let ty = drop_params(&u.funs[&k.0].ty, &k.1);
        let span = p
            .binds
            .iter()
            .find(|b| b.name == k.0)
            .map(|b| b.span)
            .unwrap_or(cirka_diagnostics::Span::DUMMY);
        new.push(TopBind {
            name: names[k].clone(),
            ty,
            expr,
            span,
        });
    }
    for b in &mut p.binds {
        let e = std::mem::replace(&mut b.expr, Expr::Lit(Lit::Unit));
        b.expr = u.rewrite_calls(e, &root_map, &names);
    }
    p.binds.extend(new);
    accepted.len()
}

/// The recursive monomorphic functions with `Int#` parameters.
fn candidates(p: &Program) -> HashMap<String, Fun> {
    let calls: HashMap<&str, Vec<String>> = p
        .binds
        .iter()
        .map(|b| {
            let mut gs = Vec::new();
            walk(&b.expr, &mut |e| {
                if let Expr::Global(g) = e {
                    gs.push(g.clone());
                }
            });
            (b.name.as_str(), gs)
        })
        .collect();
    let mut out = HashMap::new();
    for b in &p.binds {
        let (tvs, params, body) = split_fun(&b.expr);
        if !tvs.is_empty() || !params.iter().any(|x| x.ty == Type::int()) {
            continue;
        }
        if arity(&b.ty) < params.len() || !reaches(&calls, &b.name) {
            continue;
        }
        out.insert(
            b.name.clone(),
            Fun {
                ty: b.ty.clone(),
                params,
                body: body.clone(),
            },
        );
    }
    out
}

/// The number of arrows of a function type.
fn arity(t: &Type) -> usize {
    match t {
        Type::Fun(_, _, r) => 1 + arity(r),
        _ => 0,
    }
}

/// Whether `f` calls itself, directly or through other globals.
fn reaches(calls: &HashMap<&str, Vec<String>>, f: &str) -> bool {
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = calls
        .get(f)
        .map(|v| v.iter().map(String::as_str).collect())
        .unwrap_or_default();
    while let Some(g) = stack.pop() {
        if g == f {
            return true;
        }
        if seen.insert(g)
            && let Some(v) = calls.get(g)
        {
            stack.extend(v.iter().map(String::as_str));
        }
    }
    false
}

/// `t` without the parameters that have literals.
fn drop_params(t: &Type, lits: &[Option<i64>]) -> Type {
    match (t, lits.split_first()) {
        (Type::Fun(m, a, r), Some((l, rest))) => {
            let r = drop_params(r, rest);
            if l.is_some() {
                r
            } else {
                Type::fun(*m, (**a).clone(), r)
            }
        }
        _ => t.clone(),
    }
}

/// The name of a copy: the literals of the `Int#` parameters (`x` for one
/// that stays a parameter, `m` for minus).
fn copy_name(u: &Unroller, k: &Key) -> String {
    let parts: Vec<String> = u.funs[&k.0]
        .params
        .iter()
        .zip(&k.1)
        .filter(|(b, _)| b.ty == Type::int())
        .map(|(_, l)| match l {
            None => "x".to_string(),
            Some(n) if *n < 0 => format!("m{}", n.unsigned_abs()),
            Some(n) => n.to_string(),
        })
        .collect();
    format!("{}$k{}", k.0, parts.join("_"))
}

/// The calls with literal arguments in tail position of `e`, with their
/// keys.
fn tail_calls<'e>(u: &Unroller, e: &'e Expr) -> Vec<(Key, &'e Expr)> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        match e {
            Expr::Let(_, b) | Expr::LetStrict(_, _, b) | Expr::Cast(b, _) => stack.push(b),
            Expr::Case(_, _, _, alts) => stack.extend(alts.iter().map(|a| &a.rhs)),
            Expr::App(..) => out.extend(u.key_of(e).map(|k| (k, e))),
            _ => {}
        }
    }
    out
}

/// The keys of the calls with literal arguments in tail position of `e`.
fn tail_keys(u: &Unroller, e: &Expr) -> Vec<Key> {
    tail_calls(u, e).into_iter().map(|(k, _)| k).collect()
}

impl Unroller {
    /// The key of a full call of a candidate with some literal `Int#`
    /// arguments, keeping at least one parameter.
    fn key_of(&self, e: &Expr) -> Option<Key> {
        let (h, args) = e.collect_args();
        let Expr::Global(g) = h else {
            return None;
        };
        let f = self.funs.get(g)?;
        if args.len() != f.params.len() {
            return None;
        }
        let lits: Vec<Option<i64>> = f
            .params
            .iter()
            .zip(&args)
            .map(|(b, a)| match a {
                Expr::Lit(Lit::Int(n)) if b.ty == Type::int() => Some(*n),
                _ => None,
            })
            .collect();
        let n = lits.iter().filter(|l| l.is_some()).count();
        (n > 0 && n < lits.len()).then(|| (g.clone(), lits))
    }

    /// The keys of the calls in `e` with literal arguments, those in tail
    /// position only when `tails`.
    fn roots_in(&self, e: &Expr, tails: bool, out: &mut Vec<Key>) {
        match e {
            Expr::Let(Bind::NonRec(_, r), b) | Expr::LetStrict(_, r, b) => {
                self.roots_in(r, true, out);
                self.roots_in(b, tails, out);
            }
            Expr::Let(Bind::Rec(bs), b) => {
                for (_, r) in bs {
                    self.roots_in(r, true, out);
                }
                self.roots_in(b, tails, out);
            }
            Expr::Case(x, _, _, alts) => {
                self.roots_in(x, true, out);
                for a in alts {
                    self.roots_in(&a.rhs, tails, out);
                }
            }
            Expr::Cast(x, _) => self.roots_in(x, tails, out),
            Expr::App(..) => {
                if tails && let Some(k) = self.key_of(e) {
                    out.push(k);
                }
                let (h, args) = e.collect_args();
                self.roots_in(h, true, out);
                for a in args {
                    self.roots_in(a, true, out);
                }
            }
            _ => walk_children(e, &mut |c| self.roots_in(c, true, out)),
        }
    }

    /// Whether the arguments of the call `e` of the copy `k` can be bound
    /// to its parameters when it is inlined, as β-reduction binds them: an
    /// atom is substituted, an unboxed value bound by `let!`, and a lazy
    /// `let` takes a value or a parameter used any number of times.
    fn bindable(&self, k: &Key, e: &Expr) -> bool {
        let (_, args) = e.collect_args();
        self.funs[&k.0]
            .params
            .iter()
            .zip(&k.1)
            .zip(args)
            .filter(|((_, l), _)| l.is_none())
            .all(|((b, _), a)| {
                a.is_atom() || b.ty.is_unboxed() || b.mult == Mult::Many || a.is_value()
            })
    }

    /// The body of the copy `k` (cached).
    fn body(&mut self, k: &Key) -> &Expr {
        if !self.bodies.contains_key(k) {
            let f = &self.funs[&k.0];
            let mut env = HashMap::new();
            for (b, l) in f.params.iter().zip(&k.1) {
                if let Some(n) = l {
                    env.insert(b.id.id, *n);
                }
            }
            let e = reduce(f.body.clone(), &mut env);
            self.bodies.insert(k.clone(), e);
        }
        &self.bodies[k]
    }

    /// The copies the tail calls reach from `root`, or `None` when they are
    /// too many or too large (or only `root`: nothing is unrolled).
    fn closure(&mut self, root: &Key) -> Option<Vec<Key>> {
        let mut order = vec![root.clone()];
        let mut seen: HashSet<Key> = HashSet::from([root.clone()]);
        let mut nodes = 0;
        let mut i = 0;
        while i < order.len() {
            let k = order[i].clone();
            i += 1;
            let body = self.body(&k).clone();
            nodes += size(&body);
            if nodes > MAX_NODES {
                return None;
            }
            for t in tail_keys(self, &body) {
                if seen.insert(t.clone()) {
                    order.push(t);
                    if order.len() > MAX_COPIES {
                        return None;
                    }
                }
            }
        }
        (order.len() > 1).then_some(order)
    }

    /// The copies reached from the call site `root`, specialized on as many
    /// of its literals as keep them few: a literal that only counts the
    /// iterations of an outer loop (`steps b 1000`) would make a copy per
    /// iteration. Returns the key used and its copies.
    fn closure_any(&mut self, root: &Key) -> Option<(Key, Vec<Key>)> {
        let lits: Vec<usize> = (0..root.1.len()).filter(|&i| root.1[i].is_some()).collect();
        let n = lits.len().min(MAX_FORGET);
        let mut masks: Vec<u32> = (1..1u32 << n).collect();
        masks.sort_by_key(|m| std::cmp::Reverse(m.count_ones()));
        for m in masks {
            let mut g = root.clone();
            for (bit, &i) in lits.iter().take(n).enumerate() {
                if m & (1 << bit) == 0 {
                    g.1[i] = None;
                }
            }
            if let Some(ks) = self.closure(&g) {
                return Some((g, ks));
            }
        }
        None
    }

    /// The function of the copy `k`: its body with the tail calls of the
    /// copies redirected, inlining those not kept (freshened).
    fn build(
        &self,
        k: &Key,
        accepted: &HashSet<Key>,
        names: &HashMap<Key, String>,
        next: &mut u32,
    ) -> Expr {
        let f = &self.funs[&k.0];
        let body = self.tails(self.bodies[k].clone(), accepted, names, next);
        let e = f
            .params
            .iter()
            .zip(&k.1)
            .filter(|(_, l)| l.is_none())
            .rev()
            .fold(body, |e, (b, _)| Expr::Lam(b.clone(), Box::new(e)));
        freshen(&e, next)
    }

    fn tails(
        &self,
        e: Expr,
        accepted: &HashSet<Key>,
        names: &HashMap<Key, String>,
        next: &mut u32,
    ) -> Expr {
        match e {
            Expr::Let(b, x) => Expr::Let(b, Box::new(self.tails(*x, accepted, names, next))),
            Expr::LetStrict(b, r, x) => {
                Expr::LetStrict(b, r, Box::new(self.tails(*x, accepted, names, next)))
            }
            Expr::Cast(x, t) => Expr::Cast(Box::new(self.tails(*x, accepted, names, next)), t),
            Expr::Case(s, b, t, alts) => {
                let alts = alts
                    .into_iter()
                    .map(|a| Alt {
                        con: a.con,
                        rhs: self.tails(a.rhs, accepted, names, next),
                    })
                    .collect();
                Expr::Case(s, b, t, alts)
            }
            Expr::App(..) => match self.key_of(&e) {
                Some(k) if accepted.contains(&k) => match names.get(&k) {
                    Some(n) => call(Expr::Global(n.clone()), &e, &k),
                    None => self.inline(&k, &e, accepted, names, next),
                },
                _ => e,
            },
            e => e,
        }
    }

    /// The copy `k` inlined at its tail call `e`: the body with the
    /// parameters bound to the arguments ([`Self::bindable`]). A β-redex
    /// would leave an unboxed tuple the copy returns outside a return
    /// position until the simplifier reduces it.
    fn inline(
        &self,
        k: &Key,
        e: &Expr,
        accepted: &HashSet<Key>,
        names: &HashMap<Key, String>,
        next: &mut u32,
    ) -> Expr {
        let mut body = self.build(k, accepted, names, next);
        let mut binds = Vec::new();
        for (a, _) in e
            .collect_args()
            .1
            .into_iter()
            .zip(&k.1)
            .filter(|(_, l)| l.is_none())
        {
            let Expr::Lam(b, x) = body else {
                unreachable!("a copy takes the arguments that are not literals")
            };
            binds.push((b, a.clone()));
            body = *x;
        }
        for (b, a) in binds.into_iter().rev() {
            body = if a.is_atom() {
                replace_var(body, b.id.id, &a)
            } else if b.ty.is_unboxed() {
                Expr::LetStrict(b, Box::new(a), Box::new(body))
            } else {
                Expr::Let(Bind::NonRec(b, Box::new(a)), Box::new(body))
            };
        }
        body
    }

    /// `e` with the call sites redirected to the copies they were
    /// specialized to (`root_map`).
    fn rewrite_calls(
        &self,
        e: Expr,
        root_map: &HashMap<Key, Key>,
        names: &HashMap<Key, String>,
    ) -> Expr {
        let e = map_children(e, &mut |c| self.rewrite_calls(c, root_map, names));
        match self.key_of(&e).and_then(|k| root_map.get(&k)) {
            Some(g) => call(Expr::Global(names[g].clone()), &e, g),
            None => e,
        }
    }
}

/// `f` applied to each direct sub-expression of `e`.
fn walk_children(e: &Expr, f: &mut impl FnMut(&Expr)) {
    match e {
        Expr::Var(_) | Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
        Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => a.iter().for_each(f),
        Expr::App(g, a) => {
            f(g);
            f(a);
        }
        Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) | Expr::Lam(_, x) => f(x),
        Expr::Let(Bind::NonRec(_, r), b) | Expr::LetStrict(_, r, b) => {
            f(r);
            f(b);
        }
        Expr::Let(Bind::Rec(bs), b) => {
            bs.iter().for_each(|(_, r)| f(r));
            f(b);
        }
        Expr::Case(s, _, _, alts) => {
            f(s);
            alts.iter().for_each(|a| f(&a.rhs));
        }
    }
}

/// `head` applied to the arguments of the call `e` that have no literal in
/// `k`.
fn call(head: Expr, e: &Expr, k: &Key) -> Expr {
    let (_, args) = e.collect_args();
    let rest: Vec<Expr> = args
        .into_iter()
        .zip(&k.1)
        .filter(|(_, l)| l.is_none())
        .map(|(a, _)| a.clone())
        .collect();
    Expr::app(head, rest)
}

/// `e` with the variables of `env` replaced by their literals, the
/// primitives on literals folded and the `case`s on literals decided.
/// Binders are unique, so `env` needs no scopes.
fn reduce(e: Expr, env: &mut HashMap<u32, i64>) -> Expr {
    match e {
        Expr::Var(x) => match env.get(&x.id) {
            Some(n) => Expr::int(*n),
            None => Expr::Var(x),
        },
        Expr::Prim(op, ts, args) => {
            let args: Vec<Expr> = args.into_iter().map(|a| reduce(a, env)).collect();
            match fold(op, &args) {
                Some(l @ Lit::Int(_)) => Expr::Lit(l),
                _ => Expr::Prim(op, ts, args),
            }
        }
        Expr::LetStrict(b, r, x) => {
            let r = reduce(*r, env);
            if let Expr::Lit(Lit::Int(n)) = r
                && b.ty == Type::int()
            {
                env.insert(b.id.id, n);
                return reduce(*x, env);
            }
            Expr::LetStrict(b, Box::new(r), Box::new(reduce(*x, env)))
        }
        Expr::Case(s, b, t, alts) => {
            let s = reduce(*s, env);
            if let Expr::Lit(Lit::Int(n)) = s {
                let pick = alts
                    .iter()
                    .position(|a| a.con == AltCon::Int(n))
                    .or_else(|| alts.iter().position(|a| a.con == AltCon::Default));
                if let Some(i) = pick {
                    env.insert(b.id.id, n);
                    let rhs = alts.into_iter().nth(i).expect("picked").rhs;
                    return reduce(rhs, env);
                }
            }
            let alts = alts
                .into_iter()
                .map(|a| Alt {
                    con: a.con,
                    rhs: reduce(a.rhs, env),
                })
                .collect();
            Expr::Case(Box::new(s), b, t, alts)
        }
        e => map_children(e, &mut |c| reduce(c, env)),
    }
}
