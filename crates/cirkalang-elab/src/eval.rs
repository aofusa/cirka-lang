//! Normalization by evaluation: evaluation, quoting, weak-head
//! normalization with on-demand unfolding of definitions, and unification
//! with pattern-unification of metavariables.

use crate::tt::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Metavariable solutions.
#[derive(Default)]
pub struct Metas {
    pub sol: RefCell<Vec<Option<Val>>>,
}

impl Metas {
    pub fn fresh(&self) -> MetaId {
        let mut s = self.sol.borrow_mut();
        s.push(None);
        (s.len() - 1) as MetaId
    }

    pub fn get(&self, m: MetaId) -> Option<Val> {
        self.sol.borrow().get(m as usize).cloned().flatten()
    }
}

/// Default evaluation budget per definition (spec §4.1).
pub const FUEL: u64 = 1_000_000;

pub struct Ev<'a> {
    pub prog: &'a Program,
    pub metas: &'a Metas,
    pub fuel: &'a Cell<u64>,
    pub out_of_fuel: &'a Cell<bool>,
}

#[derive(Debug)]
pub enum UnifyError {
    Mismatch,
    /// Meta solution failed (occurs/scope check).
    Meta,
}

impl<'a> Ev<'a> {
    pub fn new(
        prog: &'a Program,
        metas: &'a Metas,
        fuel: &'a Cell<u64>,
        out_of_fuel: &'a Cell<bool>,
    ) -> Ev<'a> {
        Ev {
            prog,
            metas,
            fuel,
            out_of_fuel,
        }
    }

    fn tick(&self) -> bool {
        let f = self.fuel.get();
        if f == 0 {
            self.out_of_fuel.set(true);
            return false;
        }
        self.fuel.set(f - 1);
        true
    }

    // ---- evaluation ----

    pub fn eval(&self, env: &Env, t: &Term) -> Val {
        match t {
            Term::Var(i) => {
                let n = env.len() as i64 - 1 - *i as i64;
                if n < 0 {
                    return Val::Error;
                }
                env[n as usize].clone()
            }
            Term::Global(g) => self.global(*g),
            Term::Type => Val::Type,
            Term::LType => Val::LType,
            Term::Lit(l) => Val::Lit(l.clone()),
            Term::Error => Val::Error,
            Term::Pi(x, m, i, a, b) => Val::Pi(
                x.clone(),
                *m,
                *i,
                Rc::new(self.eval(env, a)),
                Closure {
                    env: env.clone(),
                    body: b.clone(),
                },
            ),
            Term::Lam(x, m, i, a, b) => Val::Lam(
                x.clone(),
                *m,
                *i,
                Rc::new(self.eval(env, a)),
                Closure {
                    env: env.clone(),
                    body: b.clone(),
                },
            ),
            Term::App(f, a, i) => {
                let fv = self.eval(env, f);
                let av = self.eval(env, a);
                self.apply(fv, av, *i)
            }
            Term::Let(_, _, _, v, b) => {
                let vv = self.eval(env, v);
                let mut e = env.clone();
                e.push(vv);
                self.eval(&e, b)
            }
            Term::Case(c) => {
                let s = self.eval(env, &c.scrut);
                self.case(s, env, c)
            }
            Term::Meta(m) => match self.metas.get(*m) {
                Some(v) => v,
                None => Val::Neutral(Head::Meta(*m), Vec::new()),
            },
            Term::InsertedMeta(m, mask) => {
                let mut v = match self.metas.get(*m) {
                    Some(v) => v,
                    None => Val::Neutral(Head::Meta(*m), Vec::new()),
                };
                for (l, bound) in mask.iter().enumerate() {
                    if *bound && l < env.len() {
                        v = self.apply(v, env[l].clone(), Icit::Expl);
                    }
                }
                v
            }
        }
    }

    fn global(&self, g: GName) -> Val {
        match self.prog.get(g) {
            Global::Con { .. } => Val::Con(g, Vec::new()),
            Global::Data { .. } => Val::Data(g, Vec::new()),
            _ => Val::Neutral(Head::Global(g), Vec::new()),
        }
    }

    pub fn inst(&self, c: &Closure, v: Val) -> Val {
        let mut e = c.env.clone();
        e.push(v);
        self.eval(&e, &c.body)
    }

    pub fn apply(&self, f: Val, a: Val, i: Icit) -> Val {
        match f {
            Val::Lam(_, _, _, _, c) => self.inst(&c, a),
            Val::Neutral(h, mut sp) => {
                sp.push(Elim::App(a, i));
                Val::Neutral(h, sp)
            }
            Val::Con(c, mut args) => {
                args.push(a);
                Val::Con(c, args)
            }
            Val::Data(d, mut args) => {
                args.push((a, i));
                Val::Data(d, args)
            }
            _ => Val::Error,
        }
    }

    /// Selects a case alternative, or gets stuck.
    pub fn case(&self, s: Val, env: &Env, c: &Rc<Case>) -> Val {
        match self.force(s) {
            Val::Con(k, args) => {
                let nparams = match self.prog.get(k) {
                    Global::Con { nparams, .. } => *nparams,
                    _ => 0,
                };
                for a in &c.alts {
                    if let Alt::Con { con, body, .. } = a
                        && *con == k
                    {
                        let mut e = env.clone();
                        e.extend(args.into_iter().skip(nparams));
                        return self.eval(&e, body);
                    }
                }
                match &c.default {
                    Some(d) => self.eval(env, d),
                    None => Val::Error,
                }
            }
            Val::Lit(l) => {
                for a in &c.alts {
                    if let Alt::Lit { lit, body } = a
                        && *lit == l
                    {
                        return self.eval(env, body);
                    }
                }
                match &c.default {
                    Some(d) => self.eval(env, d),
                    None => Val::Error,
                }
            }
            Val::Neutral(h, mut sp) => {
                sp.push(Elim::Case(env.clone(), c.clone()));
                Val::Neutral(h, sp)
            }
            _ => Val::Error,
        }
    }

    fn apply_elims(&self, mut v: Val, sp: Vec<Elim>) -> Val {
        for e in sp {
            v = match e {
                Elim::App(a, i) => self.apply(v, a, i),
                Elim::Case(env, c) => self.case(v, &env, &c),
            };
        }
        v
    }

    /// Replaces solved metas in the head.
    pub fn force(&self, v: Val) -> Val {
        match v {
            Val::Neutral(Head::Meta(m), sp) => match self.metas.get(m) {
                Some(s) => {
                    let r = self.apply_elims(s, sp);
                    self.force(r)
                }
                None => Val::Neutral(Head::Meta(m), sp),
            },
            v => v,
        }
    }

    /// Unfolds one definition or reduces one primitive at the head.
    pub fn unfold(&self, v: &Val) -> Option<Val> {
        let Val::Neutral(Head::Global(g), sp) = v else {
            return None;
        };
        match self.prog.get(*g) {
            Global::Def { body: Some(b), .. } => {
                if !self.tick() {
                    return Some(Val::Error);
                }
                let f = self.eval(&Vec::new(), b);
                Some(self.apply_elims(f, sp.clone()))
            }
            Global::Prim { prim, .. } => self.prim_reduce(*prim, sp),
            _ => None,
        }
    }

    /// Weak head normal form: forces metas and unfolds definitions.
    pub fn whnf(&self, v: Val) -> Val {
        let mut v = self.force(v);
        loop {
            match self.unfold(&v) {
                Some(u) => v = self.force(u),
                None => return v,
            }
        }
    }

    fn prim_reduce(&self, p: Prim, sp: &[Elim]) -> Option<Val> {
        let args: Vec<Val> = sp
            .iter()
            .map(|e| match e {
                Elim::App(a, _) => Some(self.whnf(a.clone())),
                Elim::Case(..) => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let int = |i: usize| match args.get(i) {
            Some(Val::Lit(Lit::Int(n))) => Some(*n),
            _ => None,
        };
        let b = |x: bool| {
            self.prog_builtins()
                .map(|(t, f)| Val::Con(if x { t } else { f }, Vec::new()))
        };
        let r = match (p, args.len()) {
            (Prim::AddInt, 2) => Val::Lit(Lit::Int(int(0)?.wrapping_add(int(1)?))),
            (Prim::SubInt, 2) => Val::Lit(Lit::Int(int(0)?.wrapping_sub(int(1)?))),
            (Prim::MulInt, 2) => Val::Lit(Lit::Int(int(0)?.wrapping_mul(int(1)?))),
            (Prim::NegInt, 1) => Val::Lit(Lit::Int(int(0)?.wrapping_neg())),
            (Prim::EqInt, 2) => b(int(0)? == int(1)?)?,
            (Prim::LtInt, 2) => b(int(0)? < int(1)?)?,
            (Prim::LeInt, 2) => b(int(0)? <= int(1)?)?,
            _ => return None,
        };
        Some(r)
    }

    fn prog_builtins(&self) -> Option<(GName, GName)> {
        Some((self.prog.builtins.true_?, self.prog.builtins.false_?))
    }

    // ---- quoting ----

    /// Reads back a value as a term at level `lvl` (no unfolding).
    pub fn quote(&self, lvl: u32, v: &Val) -> Term {
        self.quote_with(lvl, v, false)
    }

    /// Reads back with definitions unfolded (normal form).
    pub fn normalize(&self, lvl: u32, v: &Val) -> Term {
        self.quote_with(lvl, v, true)
    }

    fn quote_with(&self, lvl: u32, v: &Val, unfold: bool) -> Term {
        let v = if unfold {
            self.whnf(v.clone())
        } else {
            self.force(v.clone())
        };
        match &v {
            Val::Neutral(h, sp) => {
                let mut t = match h {
                    Head::Var(x) => Term::Var(lvl.saturating_sub(x + 1)),
                    Head::Meta(m) => Term::Meta(*m),
                    Head::Global(g) => Term::Global(*g),
                };
                for e in sp {
                    t = match e {
                        Elim::App(a, i) => {
                            Term::App(Rc::new(t), Rc::new(self.quote_with(lvl, a, unfold)), *i)
                        }
                        Elim::Case(env, c) => {
                            Term::Case(Rc::new(self.quote_case(lvl, t, env, c, unfold)))
                        }
                    };
                }
                t
            }
            Val::Con(c, args) => {
                let (np, icits) = match self.prog.get(*c) {
                    Global::Con {
                        nparams,
                        field_icits,
                        ..
                    } => (*nparams, field_icits.clone()),
                    _ => (0, Vec::new()),
                };
                let mut t = Term::Global(*c);
                for (k, a) in args.iter().enumerate() {
                    let i = if k < np {
                        Icit::Impl
                    } else {
                        icits.get(k - np).copied().unwrap_or(Icit::Expl)
                    };
                    t = Term::App(Rc::new(t), Rc::new(self.quote_with(lvl, a, unfold)), i);
                }
                t
            }
            Val::Data(d, args) => {
                let mut t = Term::Global(*d);
                for (a, i) in args {
                    t = Term::App(Rc::new(t), Rc::new(self.quote_with(lvl, a, unfold)), *i);
                }
                t
            }
            Val::Lam(x, m, i, a, c) => Term::Lam(
                x.clone(),
                *m,
                *i,
                Rc::new(self.quote_with(lvl, a, unfold)),
                Rc::new(self.quote_with(lvl + 1, &self.inst(c, Val::var(lvl)), unfold)),
            ),
            Val::Pi(x, m, i, a, c) => Term::Pi(
                x.clone(),
                *m,
                *i,
                Rc::new(self.quote_with(lvl, a, unfold)),
                Rc::new(self.quote_with(lvl + 1, &self.inst(c, Val::var(lvl)), unfold)),
            ),
            Val::Type => Term::Type,
            Val::LType => Term::LType,
            Val::Lit(l) => Term::Lit(l.clone()),
            Val::Error => Term::Error,
        }
    }

    fn quote_case(&self, lvl: u32, scrut: Term, env: &Env, c: &Case, unfold: bool) -> Case {
        let ret = self.quote_with(lvl, &self.eval(env, &c.ret), unfold);
        let alts = c
            .alts
            .iter()
            .map(|a| match a {
                Alt::Con {
                    con, fields, body, ..
                } => {
                    let mut e = env.clone();
                    let mut fs = Vec::new();
                    for (k, (x, m, ty)) in fields.iter().enumerate() {
                        let tv = self.eval(&e, ty);
                        fs.push((x.clone(), *m, self.quote_with(lvl + k as u32, &tv, unfold)));
                        e.push(Val::var(lvl + k as u32));
                    }
                    let b = self.eval(&e, body);
                    Alt::Con {
                        con: *con,
                        fields: fs,
                        refine: Vec::new(),
                        body: self.quote_with(lvl + fields.len() as u32, &b, unfold),
                    }
                }
                Alt::Lit { lit, body } => Alt::Lit {
                    lit: lit.clone(),
                    body: self.quote_with(lvl, &self.eval(env, body), unfold),
                },
            })
            .collect();
        let default = c
            .default
            .as_ref()
            .map(|d| self.quote_with(lvl, &self.eval(env, d), unfold));
        Case {
            scrut,
            ret,
            alts,
            default,
        }
    }

    // ---- unification ----

    pub fn unify(&self, lvl: u32, a: &Val, b: &Val) -> Result<(), UnifyError> {
        let a = self.force(a.clone());
        let b = self.force(b.clone());
        match self.unify_shallow(lvl, &a, &b) {
            Ok(()) => Ok(()),
            Err(e) => {
                // Unfold definitions and try again.
                let ua = self.unfold(&a);
                let ub = self.unfold(&b);
                match (ua, ub) {
                    (None, None) => Err(e),
                    (Some(x), None) => self.unify(lvl, &x, &b),
                    (None, Some(y)) => self.unify(lvl, &a, &y),
                    (Some(x), Some(y)) => self.unify(lvl, &x, &y),
                }
            }
        }
    }

    fn unify_shallow(&self, lvl: u32, a: &Val, b: &Val) -> Result<(), UnifyError> {
        use Val::*;
        match (a, b) {
            (Error, _) | (_, Error) => Ok(()),
            (Type, Type) | (LType, LType) => Ok(()),
            (Lit(x), Lit(y)) if x == y => Ok(()),
            (Lam(_, _, _, _, c1), Lam(_, _, _, _, c2)) => {
                let x = Val::var(lvl);
                self.unify(lvl + 1, &self.inst(c1, x.clone()), &self.inst(c2, x))
            }
            (Lam(_, _, i, _, c), f) | (f, Lam(_, _, i, _, c)) => {
                let x = Val::var(lvl);
                self.unify(
                    lvl + 1,
                    &self.inst(c, x.clone()),
                    &self.apply(f.clone(), x, *i),
                )
            }
            (Pi(_, m1, i1, a1, c1), Pi(_, m2, i2, a2, c2)) => {
                if m1 != m2 || i1 != i2 {
                    return Err(UnifyError::Mismatch);
                }
                self.unify(lvl, a1, a2)?;
                let x = Val::var(lvl);
                self.unify(lvl + 1, &self.inst(c1, x.clone()), &self.inst(c2, x))
            }
            (Con(c1, a1), Con(c2, a2)) if c1 == c2 && a1.len() == a2.len() => {
                for (x, y) in a1.iter().zip(a2) {
                    self.unify(lvl, x, y)?;
                }
                Ok(())
            }
            (Data(d1, a1), Data(d2, a2)) if d1 == d2 && a1.len() == a2.len() => {
                for ((x, _), (y, _)) in a1.iter().zip(a2) {
                    self.unify(lvl, x, y)?;
                }
                Ok(())
            }
            (Neutral(Head::Meta(m1), s1), Neutral(Head::Meta(m2), s2)) if m1 == m2 => {
                self.unify_spines(lvl, s1, s2)
            }
            (Neutral(Head::Meta(m), sp), t) | (t, Neutral(Head::Meta(m), sp)) => {
                self.solve(lvl, *m, sp, t)
            }
            (Neutral(h1, s1), Neutral(h2, s2)) if h1 == h2 => self.unify_spines(lvl, s1, s2),
            _ => Err(UnifyError::Mismatch),
        }
    }

    fn unify_spines(&self, lvl: u32, s1: &[Elim], s2: &[Elim]) -> Result<(), UnifyError> {
        if s1.len() != s2.len() {
            return Err(UnifyError::Mismatch);
        }
        for (x, y) in s1.iter().zip(s2) {
            match (x, y) {
                (Elim::App(a, _), Elim::App(b, _)) => self.unify(lvl, a, b)?,
                (Elim::Case(e1, c1), Elim::Case(e2, c2)) => {
                    let t1 = self.quote_case(lvl, Term::Error, e1, c1, false);
                    let t2 = self.quote_case(lvl, Term::Error, e2, c2, false);
                    if !case_eq(&t1, &t2) {
                        return Err(UnifyError::Mismatch);
                    }
                }
                _ => return Err(UnifyError::Mismatch),
            }
        }
        Ok(())
    }

    /// Solves `?m sp =? rhs` by pattern unification.
    fn solve(&self, lvl: u32, m: MetaId, sp: &[Elim], rhs: &Val) -> Result<(), UnifyError> {
        let mut ren: HashMap<u32, u32> = HashMap::new();
        let mut icits = Vec::new();
        for e in sp {
            let Elim::App(a, i) = e else {
                return Err(UnifyError::Meta);
            };
            match self.force(a.clone()) {
                Val::Neutral(Head::Var(x), s) if s.is_empty() && !ren.contains_key(&x) => {
                    let d = ren.len() as u32;
                    ren.insert(x, d);
                    icits.push(*i);
                }
                _ => return Err(UnifyError::Meta),
            }
        }
        let dom = ren.len() as u32;
        let body = self.rename(m, dom, lvl, &ren, rhs)?;
        let mut t = body;
        for (k, i) in icits.iter().enumerate().rev() {
            t = Term::Lam(
                format!("x{k}").into(),
                Mult::Many,
                *i,
                Rc::new(Term::Error),
                Rc::new(t),
            );
        }
        let v = self.eval(&Vec::new(), &t);
        self.metas.sol.borrow_mut()[m as usize] = Some(v);
        Ok(())
    }

    /// Renames `v` (at level `cod`) into the solution context of `dom`
    /// variables, with occurs and scope checks.
    fn rename(
        &self,
        m: MetaId,
        dom: u32,
        cod: u32,
        ren: &HashMap<u32, u32>,
        v: &Val,
    ) -> Result<Term, UnifyError> {
        let v = self.force(v.clone());
        let lift = |ren: &HashMap<u32, u32>| {
            let mut r = ren.clone();
            r.insert(cod, dom);
            r
        };
        Ok(match &v {
            Val::Neutral(h, sp) => {
                let mut t = match h {
                    Head::Meta(m2) if *m2 == m => return Err(UnifyError::Meta),
                    Head::Meta(m2) => Term::Meta(*m2),
                    Head::Var(x) => match ren.get(x) {
                        Some(y) => Term::Var(dom - 1 - y),
                        None => return Err(UnifyError::Meta),
                    },
                    Head::Global(g) => Term::Global(*g),
                };
                for e in sp {
                    t = match e {
                        Elim::App(a, i) => {
                            Term::App(Rc::new(t), Rc::new(self.rename(m, dom, cod, ren, a)?), *i)
                        }
                        Elim::Case(..) => return Err(UnifyError::Meta),
                    };
                }
                t
            }
            Val::Con(c, args) => {
                let (np, icits) = match self.prog.get(*c) {
                    Global::Con {
                        nparams,
                        field_icits,
                        ..
                    } => (*nparams, field_icits.clone()),
                    _ => (0, Vec::new()),
                };
                let mut t = Term::Global(*c);
                for (k, a) in args.iter().enumerate() {
                    let i = if k < np {
                        Icit::Impl
                    } else {
                        icits.get(k - np).copied().unwrap_or(Icit::Expl)
                    };
                    t = Term::App(Rc::new(t), Rc::new(self.rename(m, dom, cod, ren, a)?), i);
                }
                t
            }
            Val::Data(d, args) => {
                let mut t = Term::Global(*d);
                for (a, i) in args {
                    t = Term::App(Rc::new(t), Rc::new(self.rename(m, dom, cod, ren, a)?), *i);
                }
                t
            }
            Val::Lam(x, mm, i, a, c) => Term::Lam(
                x.clone(),
                *mm,
                *i,
                Rc::new(self.rename(m, dom, cod, ren, a)?),
                Rc::new(self.rename(
                    m,
                    dom + 1,
                    cod + 1,
                    &lift(ren),
                    &self.inst(c, Val::var(cod)),
                )?),
            ),
            Val::Pi(x, mm, i, a, c) => Term::Pi(
                x.clone(),
                *mm,
                *i,
                Rc::new(self.rename(m, dom, cod, ren, a)?),
                Rc::new(self.rename(
                    m,
                    dom + 1,
                    cod + 1,
                    &lift(ren),
                    &self.inst(c, Val::var(cod)),
                )?),
            ),
            Val::Type => Term::Type,
            Val::LType => Term::LType,
            Val::Lit(l) => Term::Lit(l.clone()),
            Val::Error => Term::Error,
        })
    }
}

/// Structural equality of quoted cases (names ignored).
fn case_eq(a: &Case, b: &Case) -> bool {
    term_eq(&a.ret, &b.ret)
        && a.alts.len() == b.alts.len()
        && a.alts.iter().zip(&b.alts).all(|(x, y)| match (x, y) {
            (
                Alt::Con {
                    con: c1, body: b1, ..
                },
                Alt::Con {
                    con: c2, body: b2, ..
                },
            ) => c1 == c2 && term_eq(b1, b2),
            (Alt::Lit { lit: l1, body: b1 }, Alt::Lit { lit: l2, body: b2 }) => {
                l1 == l2 && term_eq(b1, b2)
            }
            _ => false,
        })
        && match (&a.default, &b.default) {
            (Some(x), Some(y)) => term_eq(x, y),
            (None, None) => true,
            _ => false,
        }
}

/// Alpha-equivalence of terms (binder names and annotations of lambdas
/// ignored).
pub fn term_eq(a: &Term, b: &Term) -> bool {
    use Term::*;
    match (a, b) {
        (Var(x), Var(y)) => x == y,
        (Global(x), Global(y)) => x == y,
        (Type, Type) | (LType, LType) | (Error, Error) => true,
        (Lit(x), Lit(y)) => x == y,
        (Meta(x), Meta(y)) => x == y,
        (Pi(_, m1, i1, a1, b1), Pi(_, m2, i2, a2, b2)) => {
            m1 == m2 && i1 == i2 && term_eq(a1, a2) && term_eq(b1, b2)
        }
        (Lam(_, _, i1, _, b1), Lam(_, _, i2, _, b2)) => i1 == i2 && term_eq(b1, b2),
        (App(f1, a1, _), App(f2, a2, _)) => term_eq(f1, f2) && term_eq(a1, a2),
        (Let(_, _, _, v1, b1), Let(_, _, _, v2, b2)) => term_eq(v1, v2) && term_eq(b1, b2),
        (Case(c1), Case(c2)) => term_eq(&c1.scrut, &c2.scrut) && case_eq(c1, c2),
        _ => false,
    }
}
