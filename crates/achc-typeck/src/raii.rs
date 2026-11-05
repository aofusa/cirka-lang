//! RAII and Strict-Linear on the typed program (FEAT-0122,
//! 03-type-system.md §3 and §6).
//!
//! The linearity checker decides where a linear-only value left unused is
//! given up ([`linear::Plan`]); inference marked the candidate expressions
//! with their spans (`tast::Expr::Mark`). This pass removes every mark and
//!
//! * puts `dropThen x e` before a marked expression `e` (Weaken-Drop),
//! * wraps the last statement `e` of an `IO` block in `dropAfter e x` /
//!   `releaseAfter e x` (Weaken-Release), innermost binding first,
//! * makes `let` bindings strict when their type is linear-only or their
//!   right-hand side uses a linear variable, and binds a linear-only
//!   argument that is not a variable strictly before the call.

use std::collections::HashMap;

use cirka_diagnostics::Span;

use crate::infer::Tc;
use crate::linear::{DROP, Insertion, Plan, RELEASE};
use crate::tast::{self, BindGroup, Expr};
use crate::types::*;

pub(crate) fn apply(tc: &mut Tc, plan: &Plan, binds: &mut [BindGroup]) {
    let mut at: HashMap<Span, Vec<&Insertion>> = HashMap::new();
    for i in &plan.insertions {
        at.entry(i.span).or_default().push(i);
    }
    let mut p = Pass {
        tc,
        at,
        plan,
        fresh: 0,
    };
    for g in binds.iter_mut() {
        p.group(g);
    }
}

struct Pass<'a> {
    tc: &'a mut Tc,
    at: HashMap<Span, Vec<&'a Insertion>>,
    plan: &'a Plan,
    fresh: u32,
}

impl Pass<'_> {
    fn group(&mut self, g: &mut BindGroup) {
        let bs: &mut [tast::Bind] = match g {
            BindGroup::NonRec(b) => std::slice::from_mut(b),
            BindGroup::Rec(bs) => bs,
        };
        let rec = bs.len() > 1;
        for b in bs {
            self.expr(&mut b.rhs);
            if !rec && (self.plan.strict_lets.contains(&b.name) || self.tc.is_ltype(&b.ty)) {
                b.strict = true;
            }
        }
    }

    fn rhs(&mut self, r: &mut tast::Rhs) {
        for g in r.wheres.iter_mut() {
            self.group(g);
        }
        for (gs, e) in r.body.iter_mut() {
            for g in gs.iter_mut() {
                match g {
                    tast::Guard::Bool(e) | tast::Guard::Bind(_, e) => self.expr(e),
                    tast::Guard::Let(bs) => {
                        for b in bs.iter_mut() {
                            self.group(b);
                        }
                    }
                }
            }
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        match e {
            Expr::Local(..)
            | Expr::Global(..)
            | Expr::Con(..)
            | Expr::Lit(_)
            | Expr::Error(..)
            | Expr::Hole(_)
            | Expr::OverLit(..) => {}
            Expr::App(f, a) => {
                self.expr(f);
                // Strict-Linear: a linear-only argument is evaluated before
                // the call (no thunk captures it).
                let strict_arg = match &**a {
                    Expr::Mark(_, x, t)
                        if !matches!(**x, Expr::Local(..) | Expr::Global(..) | Expr::Con(..))
                            && self.tc.is_ltype(t) =>
                    {
                        Some(t.clone())
                    }
                    _ => None,
                };
                self.expr(a);
                if let Some(t) = strict_arg {
                    self.fresh += 1;
                    let v = format!("$lin{}", self.fresh);
                    let rhs = std::mem::replace(&mut **a, Expr::Local(v.clone(), t.clone()));
                    let call = std::mem::replace(e, Expr::Lit(tast::Lit::Char(' ')));
                    *e = Expr::Let(
                        vec![BindGroup::NonRec(tast::Bind {
                            name: v,
                            tvs: Vec::new(),
                            ty: t,
                            rhs,
                            span: Span::default(),
                            strict: true,
                        })],
                        Box::new(call),
                    );
                }
            }
            Expr::Lam(_, _, b) | Expr::Coerce(b, _) => self.expr(b),
            Expr::Let(gs, b) => {
                for g in gs.iter_mut() {
                    self.group(g);
                }
                self.expr(b);
            }
            Expr::Match(_, cs, _) => {
                for c in cs.iter_mut() {
                    self.rhs(&mut c.rhs);
                }
            }
            Expr::Mark(sp, x, t) => {
                self.expr(x);
                let (sp, t) = (*sp, t.clone());
                let mut body = std::mem::replace(&mut **x, Expr::Lit(tast::Lit::Char(' ')));
                if let Some(ins) = self.at.get(&sp).cloned() {
                    // Before: in binding order, the outermost first. After:
                    // recorded innermost first (reverse binding order).
                    for i in ins.iter().filter(|i| !i.after) {
                        body = self.call(
                            "Cirka.Ownership.dropThen",
                            DROP,
                            &i.ty,
                            vec![Expr::Local(i.var.clone(), i.ty.clone()), body],
                            &t,
                        );
                    }
                    for i in ins.iter().filter(|i| i.after) {
                        let (name, class) = if i.release {
                            ("Cirka.Ownership.releaseAfter", RELEASE)
                        } else {
                            ("Cirka.Ownership.dropAfter", DROP)
                        };
                        body = self.call(
                            name,
                            class,
                            &i.ty,
                            vec![body, Expr::Local(i.var.clone(), i.ty.clone())],
                            &t,
                        );
                    }
                }
                *e = body;
            }
        }
    }

    /// `name @inst dict args..` for the library helpers
    /// (`forall (a :: LType) b. C a => ...`) at the linear-only type `lt`
    /// and the result type `res`.
    fn call(&mut self, name: &str, class: &str, lt: &Ty, args: Vec<Expr>, res: &Ty) -> Expr {
        let Some(s) = self.tc.globals.get(name).cloned() else {
            return Expr::Error(format!("internal: {name} is not loaded"), res.clone());
        };
        // Both helpers quantify the linear-only type first, then the
        // result's type.
        let res = self.tc.zonk(res);
        let other = match name {
            "Cirka.Ownership.dropThen" => res.clone(),
            _ => io_arg(&res),
        };
        let inst = vec![lt.clone(), other];
        let Some(d) = self.tc.closed_instance(class, lt) else {
            return Expr::Error(format!("internal: no {class} instance"), res.clone());
        };
        debug_assert_eq!(s.vars.len(), inst.len());
        let mut e = Expr::App(Box::new(Expr::Global(name.to_string(), inst)), Box::new(d));
        for a in args {
            e = Expr::App(Box::new(e), Box::new(a));
        }
        e
    }
}

/// `r` of `IO r`.
fn io_arg(t: &Ty) -> Ty {
    match t {
        Ty::App(_, r) => (**r).clone(),
        _ => t.clone(),
    }
}
