//! Specialization of overloaded functions (FEAT-0112).
//!
//! A call `f @τ̄ d₁ … dₖ e…` of a top-level function whose leading value
//! parameters are type class dictionaries, with closed types `τ̄` and
//! constant dictionaries `dᵢ` (globals bound to a dictionary constructor),
//! becomes a call `f$sN e…` of a copy of `f` with the types and
//! dictionaries substituted. The simplifier then resolves the method
//! selections, and the strictness analysis and worker/wrapper see the
//! monomorphic arithmetic (GHC's `SPECIALISE`, done automatically).

use crate::expr::*;
use crate::opt::{freshen, map_children, replace_var, size, subst_ty};
use crate::ty::*;
use crate::ww::split_fun;
use std::collections::HashMap;

/// At most this many specializations per program.
const MAX_SPECS: usize = 400;
/// Functions larger than this (Core nodes) are not copied.
const MAX_SIZE: usize = 4000;

/// Specializes calls with constant dictionaries; returns the number of
/// specialized copies made.
pub fn specialize(p: &mut Program, next: &mut u32) -> usize {
    let mut s = Spec {
        specs: HashMap::new(),
        names: p.binds.iter().map(|b| b.name.clone()).collect(),
        new: Vec::new(),
    };
    let mut made = 0;
    let mut i = 0;
    while i < p.binds.len() {
        let e = p.binds[i].expr.clone();
        let e = s.expr(p, e);
        p.binds[i].expr = e;
        for (name, ty, expr) in std::mem::take(&mut s.new) {
            made += 1;
            let span = p.binds[i].span;
            let expr = freshen(&expr, next);
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

struct Spec {
    /// `(function, types, dictionaries)` → specialized copy.
    specs: HashMap<String, String>,
    names: std::collections::HashSet<String>,
    new: Vec<(String, Type, Expr)>,
}

/// The dictionary bound to global `g`, if it is one.
fn is_dict(p: &Program, g: &str) -> bool {
    matches!(p.bind(g).map(|b| &b.expr), Some(Expr::Con(k, _, _)) if k.ends_with("$Dict"))
}

fn closed(t: &Type) -> bool {
    let mut v = Vec::new();
    t.free_vars(&mut v);
    v.is_empty()
}

impl Spec {
    fn expr(&mut self, p: &Program, e: Expr) -> Expr {
        let e = map_children(e, &mut |c| self.expr(p, c));
        if !matches!(e, Expr::App(..)) {
            return e;
        }
        match self.call(p, &e) {
            Some(x) => x,
            None => e,
        }
    }

    /// The specialized call replacing `e`, if any.
    fn call(&mut self, p: &Program, e: &Expr) -> Option<Expr> {
        let (h, args) = e.collect_args();
        let mut ts = Vec::new();
        let mut g = h;
        while let Expr::TyApp(x, t) = g {
            ts.push(t.clone());
            g = x;
        }
        ts.reverse();
        let Expr::Global(f) = g else { return None };
        let fb = p.bind(f)?;
        let (tvs, xs, _) = split_fun(&fb.expr);
        if tvs.len() != ts.len() || !ts.iter().all(closed) {
            return None;
        }
        let k = args
            .iter()
            .zip(&xs)
            .take_while(|(a, _)| matches!(a, Expr::Global(d) if is_dict(p, d)))
            .count();
        if k == 0 || self.specs.len() >= MAX_SPECS {
            return None;
        }
        let dicts: Vec<&str> = args[..k]
            .iter()
            .map(|a| match a {
                Expr::Global(d) => d.as_str(),
                _ => unreachable!(),
            })
            .collect();
        let key = format!(
            "{f}|{}|{}",
            ts.iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(","),
            dicts.join(",")
        );
        let name = match self.specs.get(&key) {
            Some(n) => n.clone(),
            None => {
                if size(&fb.expr) > MAX_SIZE {
                    return None;
                }
                let ty = spec_type(&fb.ty, &ts, k)?;
                // Strip the `Λ`s and the dictionary `λ`s, substituting.
                let mut body = fb.expr.clone();
                for _ in &tvs {
                    let Expr::TyLam(_, _, b) = body else {
                        return None;
                    };
                    body = *b;
                }
                for ((v, _), t) in tvs.iter().zip(&ts) {
                    body = subst_ty(&body, v, t);
                }
                for d in &dicts {
                    let Expr::Lam(x, b) = body else { return None };
                    body = replace_var(*b, x.id.id, &Expr::Global(d.to_string()));
                }
                // Registered only once the copy exists.
                let mut n = 1;
                let mut name = format!("{f}$s{n}");
                while self.names.contains(&name) {
                    n += 1;
                    name = format!("{f}$s{n}");
                }
                self.names.insert(name.clone());
                self.specs.insert(key, name.clone());
                self.new.push((name.clone(), ty, body));
                name
            }
        };
        Some(Expr::app(
            Expr::Global(name),
            args[k..].iter().map(|a| (*a).clone()),
        ))
    }
}

/// The type of `f @ts` without its first `k` parameters.
fn spec_type(t: &Type, ts: &[Type], k: usize) -> Option<Type> {
    let mut t = t.clone();
    for ty in ts {
        let Type::Forall(v, _, b) = t else {
            return None;
        };
        t = b.subst(&v, ty);
    }
    for _ in 0..k {
        let Type::Fun(_, _, r) = t else { return None };
        t = *r;
    }
    Some(t)
}
