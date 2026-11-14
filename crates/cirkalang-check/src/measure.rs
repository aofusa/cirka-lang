//! Termination by a measure (spec-v0.0.4.md §1.3): at every call of a
//! function to itself, the measure of the arguments is smaller than the
//! measure of the parameters (and, for `Int`, the latter is not negative),
//! under what the path to the call has learnt. The obligations are decided
//! by Z3. `Int` is treated as the mathematical integers.

use crate::walk::{Cond, Facts, SelfCall, Sort};
use cirka_smt::{Expr, Outcome, Query, Solver};
use cirkalang_elab::eval::Ev;
use cirkalang_elab::tt::*;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

/// The measure of a definition.
pub enum Measure<'a> {
    /// Written with `decreasing`.
    Given(&'a cirkalang_elab::tt::Measure),
    /// The parameter at this position (tried automatically).
    Param(usize),
}

/// Why a measure does not show termination.
pub enum Failure {
    /// A counterexample to an obligation (variable = value).
    Counterexample(Vec<(String, String)>),
    Unknown(String),
    /// A self-call that is not applied to all the parameters.
    Partial,
}

/// Translation of values to SMT terms. Unknown values become opaque
/// variables (`t$n`); calls of functions with a refined result add the
/// refinement of the result as a hypothesis.
pub(crate) struct Tr<'a> {
    pub ev: &'a Ev<'a>,
    pub prog: &'a Program,
    pub sorts: &'a HashMap<u32, Sort>,
    pub names: &'a HashMap<u32, String>,
    pub nat_vars: BTreeSet<String>,
    pub opaque: HashMap<String, String>,
    /// Hypotheses learnt from refined results.
    pub extra: Vec<Expr>,
    /// The value each opaque variable stands for.
    pub opaque_vals: HashMap<String, Val>,
    /// Calls of functions with a refined result whose fields are used:
    /// the variable that stands for the result (a level no binder has).
    stand_ins: HashMap<String, Val>,
    /// The call each stand-in level stands for (to show counterexamples).
    stand_in_calls: HashMap<u32, Val>,
}

/// Unique SMT names for the variables of a definition.
pub(crate) fn smt_names(facts: &Facts) -> HashMap<u32, String> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for n in facts.names.values() {
        *count.entry(n.as_str()).or_default() += 1;
    }
    facts
        .names
        .iter()
        .map(|(l, n)| {
            let ok = count.get(n.as_str()) == Some(&1)
                && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !n.starts_with(|c: char| c.is_ascii_digit());
            (*l, if ok { n.clone() } else { format!("{n}${l}") })
        })
        .collect()
}

impl<'a> Tr<'a> {
    pub fn new(
        ev: &'a Ev<'a>,
        sorts: &'a HashMap<u32, Sort>,
        names: &'a HashMap<u32, String>,
    ) -> Tr<'a> {
        Tr {
            ev,
            prog: ev.prog,
            sorts,
            names,
            nat_vars: BTreeSet::new(),
            opaque: HashMap::new(),
            extra: Vec::new(),
            opaque_vals: HashMap::new(),
            stand_ins: HashMap::new(),
            stand_in_calls: HashMap::new(),
        }
    }

    /// The hypotheses about the variables used: `Nat`s are not negative.
    pub fn var_hyps(&self) -> Vec<Expr> {
        let mut out: Vec<Expr> = self
            .nat_vars
            .iter()
            .map(|n| Expr::le(Expr::Int(0), Expr::int_var(n.clone())))
            .collect();
        out.extend(self.extra.iter().cloned());
        out
    }

    /// `Int` variables (and the opaque values of sort `Int`) are 64-bit;
    /// `Nat` variables are not bounded.
    pub fn bound_ints(&self, q: &mut Query) {
        q.bound_int_vars(i64::MIN.into(), i64::MAX.into(), &self.nat_vars);
    }

    fn var_name(&self, l: u32) -> String {
        match self.names.get(&l) {
            Some(n) => n.clone(),
            None => format!("v{l}"),
        }
    }

    pub fn opaque(&mut self, v: &Val, s: cirka_smt::Sort) -> Expr {
        let refine = self.prog.builtins.refine;
        let key = format!(
            "{:?}{:?}",
            s,
            strip_refine(&self.ev.quote(100_000, v), refine)
        );
        let n = self.opaque.len();
        let name = self
            .opaque
            .entry(key)
            .or_insert_with(|| format!("t${n}"))
            .clone();
        let shown = match v {
            Val::Neutral(Head::Var(l), sp) if sp.is_empty() => self.stand_in_calls.get(l),
            _ => None,
        };
        self.opaque_vals
            .entry(name.clone())
            .or_insert_with(|| shown.unwrap_or(v).clone());
        Expr::Var(name, s)
    }

    pub fn int(&mut self, v: &Val, depth: u32) -> Expr {
        self.tr(v, cirka_smt::Sort::Int, depth)
    }

    pub fn boolean(&mut self, v: &Val, depth: u32) -> Expr {
        self.tr(v, cirka_smt::Sort::Bool, depth)
    }

    fn tr(&mut self, v: &Val, want: cirka_smt::Sort, depth: u32) -> Expr {
        use cirka_smt::Sort as S;
        let b = &self.prog.builtins;
        let v = self.ev.force(v.clone());
        match (&v, want) {
            (Val::Lit(Lit::Int(n)), S::Int) => return Expr::Int(*n as i128),
            (Val::Con(c, args), S::Int) if Some(*c) == b.zero && args.is_empty() => {
                return Expr::Int(0);
            }
            (Val::Con(c, args), S::Int) if Some(*c) == b.succ && args.len() == 1 => {
                return Expr::plus(self.int(&args[0], depth), Expr::Int(1));
            }
            (Val::Con(c, _), S::Bool) if Some(*c) == b.true_ => return Expr::Bool(true),
            (Val::Con(c, _), S::Bool) if Some(*c) == b.false_ => return Expr::Bool(false),
            (Val::Neutral(Head::Var(l), sp), _) if sp.is_empty() => {
                match (self.sorts.get(l), want) {
                    (Some(Sort::Int), S::Int) => return Expr::int_var(self.var_name(*l)),
                    (Some(Sort::Nat), S::Int) => {
                        let n = self.var_name(*l);
                        self.nat_vars.insert(n.clone());
                        return Expr::int_var(n);
                    }
                    (Some(Sort::Bool), S::Bool) => return Expr::bool_var(self.var_name(*l)),
                    _ => {}
                }
            }
            (Val::Neutral(Head::Global(g), sp), _)
                if sp.iter().all(|e| matches!(e, Elim::App(..))) =>
            {
                if let Global::Prim { prim, .. } = self.prog.get(*g) {
                    if *prim == Prim::Refine
                        && let Some(Elim::App(x, _)) = sp.last()
                    {
                        return self.tr(x, want, depth);
                    }
                    if let Some(e) = self.prim(*prim, sp, want, depth) {
                        return e;
                    }
                    // Sizes are not negative.
                    if want == S::Int
                        && matches!(prim, Prim::ArraySize | Prim::StrLength | Prim::UArraySize)
                    {
                        let before = self.opaque.len();
                        let e = self.opaque(&v, want);
                        if self.opaque.len() > before {
                            self.extra.push(Expr::le(Expr::Int(0), e.clone()));
                        }
                        return e;
                    }
                }
                // A call of a function with a refined result: opaque, with
                // the refinement as a hypothesis (spec-v0.0.4.md §3).
                if let Some(x) = self.stand_in(&v, depth) {
                    return self.tr(&x, want, depth + 1);
                }
            }
            (Val::Neutral(h, sp), _) if matches!(sp.last(), Some(Elim::Case(..))) => {
                if let Some(e) = self.case(h, sp, want, depth) {
                    return e;
                }
            }
            _ => {}
        }
        if depth < 24
            && let Some(u) = self.unfold(&v)
        {
            return self.tr(&u, want, depth + 1);
        }
        self.opaque(&v, want)
    }

    fn unfold(&self, v: &Val) -> Option<Val> {
        self.ev.fuel.set(10_000);
        match v {
            Val::Neutral(Head::Global(g), _)
                if matches!(self.prog.get(*g), Global::Def { body: Some(_), .. }) =>
            {
                self.ev.unfold(v)
            }
            // A stuck case whose scrutinee unfolds.
            Val::Neutral(Head::Global(_), sp) if !sp.is_empty() => {
                let n = sp.iter().position(|e| matches!(e, Elim::Case(..)))?;
                let Val::Neutral(h, _) = v else { return None };
                let head = Val::Neutral(h.clone(), sp[..n].to_vec());
                let u = self.ev.unfold(&head)?;
                let mut r = self.ev.force(u);
                for e in &sp[n..] {
                    r = match e {
                        Elim::App(a, i) => self.ev.apply(r, a.clone(), *i),
                        Elim::Case(env, c) => self.ev.case(r, env, c),
                    };
                }
                Some(r)
            }
            _ => None,
        }
    }

    fn prim(&mut self, p: Prim, sp: &[Elim], want: cirka_smt::Sort, d: u32) -> Option<Expr> {
        use cirka_smt::Sort as S;
        let args: Vec<Val> = sp
            .iter()
            .filter_map(|e| match e {
                Elim::App(v, Icit::Expl) => Some(v.clone()),
                _ => None,
            })
            .collect();
        let bin = |t: &mut Self, f: fn(Expr, Expr) -> Expr| -> Option<Expr> {
            if args.len() != 2 {
                return None;
            }
            let a = t.int(&args[0], d);
            let b = t.int(&args[1], d);
            Some(f(a, b))
        };
        // `Int` is 64-bit and wraps on overflow (BUG-0018): the results of
        // `+ - * /` and negation are wrapped; `%` cannot overflow.
        match (p, want) {
            (Prim::AddInt, S::Int) => bin(self, |a, b| Expr::wrap(Expr::plus(a, b), 64)),
            (Prim::SubInt, S::Int) => bin(self, |a, b| Expr::wrap(Expr::minus(a, b), 64)),
            (Prim::MulInt, S::Int) => bin(self, |a, b| Expr::wrap(Expr::app("*", vec![a, b]), 64)),
            (Prim::DivInt, S::Int) => bin(self, |a, b| Expr::wrap(Expr::tdiv(a, b), 64)),
            (Prim::ModInt, S::Int) => bin(self, Expr::trem),
            (Prim::NegInt, S::Int) if args.len() == 1 => {
                Some(Expr::wrap(Expr::app("-", vec![self.int(&args[0], d)]), 64))
            }
            (Prim::EqInt, S::Bool) => bin(self, Expr::eq),
            (Prim::LtInt, S::Bool) => bin(self, Expr::lt),
            (Prim::LeInt, S::Bool) => bin(self, Expr::le),
            _ => None,
        }
    }

    /// A case stuck on a boolean or an integer: `ite`.
    fn case(&mut self, h: &Head, sp: &[Elim], want: cirka_smt::Sort, d: u32) -> Option<Expr> {
        let (Elim::Case(env, c), rest) = sp.split_last()? else {
            return None;
        };
        let scrut = Val::Neutral(h.clone(), rest.to_vec());
        let b = &self.prog.builtins;
        let (t, f) = (b.true_?, b.false_?);
        let is_bool = c
            .alts
            .iter()
            .all(|a| matches!(a, Alt::Con { con, .. } if *con == t || *con == f));
        if is_bool && !c.alts.is_empty() {
            let cond = self.boolean(&scrut, d + 1);
            let tv = self.ev.case(Val::Con(t, Vec::new()), env, c);
            let fv = self.ev.case(Val::Con(f, Vec::new()), env, c);
            let te = self.tr(&tv, want, d + 1);
            let fe = self.tr(&fv, want, d + 1);
            return Some(Expr::ite(cond, te, fe));
        }
        let lits: Vec<(i64, &Term)> = c
            .alts
            .iter()
            .filter_map(|a| match a {
                Alt::Lit {
                    lit: Lit::Int(n),
                    body,
                } => Some((*n, body)),
                _ => None,
            })
            .collect();
        if lits.len() == c.alts.len() && !lits.is_empty() {
            let def = c.default.as_ref()?;
            let s = self.int(&scrut, d + 1);
            let dv = self.ev.eval(env, def);
            let mut e = self.tr(&dv, want, d + 1);
            for (n, body) in lits.into_iter().rev() {
                let bv = self.ev.eval(env, body);
                let be = self.tr(&bv, want, d + 1);
                e = Expr::ite(Expr::eq(s.clone(), Expr::Int(n as i128)), be, e);
            }
            return Some(e);
        }
        // A match on a value of a type with one constructor (a record):
        // the fields are opaque projections of the value, the same in
        // every formula that mentions them.
        if let [
            Alt::Con {
                con, fields, body, ..
            },
        ] = c.alts.as_slice()
            && let Global::Con { data, .. } = self.prog.get(*con)
            && matches!(self.prog.get(*data), Global::Data { cons, .. } if cons.len() == 1)
        {
            let n = fields.len();
            // A field itself (a projection): opaque.
            if let Term::Var(k) = body
                && (*k as usize) < n
            {
                let scrut = Val::Neutral(h.clone(), rest.to_vec());
                let scrut = self.stand_in(&scrut, d).unwrap_or(scrut);
                let v = projection(&scrut, *con, n, n - 1 - *k as usize);
                return Some(self.opaque(&v, want));
            }
            let mut e = env.clone();
            let scrut = Val::Neutral(h.clone(), rest.to_vec());
            let scrut = self.stand_in(&scrut, d).unwrap_or(scrut);
            for i in 0..n {
                e.push(projection(&scrut, *con, n, i));
            }
            let bv = self.ev.eval(&e, body);
            return Some(self.tr(&bv, want, d + 1));
        }
        None
    }

    /// A call `v` of a function with a refined result, seen through the
    /// fields of the result: a variable stands for the result, with the
    /// refinements as hypotheses. (Matching on the call itself would
    /// unfold the function.)
    pub fn stand_in(&mut self, v: &Val, depth: u32) -> Option<Val> {
        let Val::Neutral(Head::Global(g), sp) = v else {
            return None;
        };
        if depth >= 24 || !sp.iter().all(|e| matches!(e, Elim::App(..))) {
            return None;
        }
        let key = format!(
            "{:?}",
            strip_refine(&self.ev.quote(100_000, v), self.prog.builtins.refine)
        );
        if let Some(x) = self.stand_ins.get(&key) {
            return Some(x.clone());
        }
        let ps = crate::walk::result_refinements(self.prog, self.ev, *g, sp);
        if ps.is_empty() {
            return None;
        }
        // Below the level the values are quoted at, above any binder.
        let l = STAND_IN_LEVEL + self.stand_ins.len() as u32;
        let x = Val::var(l);
        self.stand_ins.insert(key, x.clone());
        self.stand_in_calls.insert(l, v.clone());
        for p in ps {
            let holds = self.ev.apply(p, x.clone(), Icit::Expl);
            let h = self.boolean(&holds, depth + 1);
            self.extra.push(h);
        }
        Some(x)
    }

    /// `v` matched the only constructor of its type: each field of a
    /// translated sort is the projection of `v`.
    fn record_cond(&mut self, v: &Val, con: GName, fields: &[u32]) -> Option<Expr> {
        let Global::Con { data, .. } = self.prog.get(con) else {
            return None;
        };
        if !matches!(self.prog.get(*data), Global::Data { cons, .. } if cons.len() == 1) {
            return None;
        }
        let scrut = self.ev.force(v.clone());
        let scrut = self.stand_in(&scrut, 0).unwrap_or(scrut);
        let n = fields.len();
        let mut eqs = Vec::new();
        for (i, l) in fields.iter().enumerate() {
            let want = match self.sorts.get(l) {
                Some(Sort::Int | Sort::Nat) => cirka_smt::Sort::Int,
                Some(Sort::Bool) => cirka_smt::Sort::Bool,
                None => continue,
            };
            let f = self.tr(&Val::var(*l), want, 0);
            let p = self.tr(&projection(&scrut, con, n, i), want, 0);
            eqs.push(Expr::eq(f, p));
        }
        if eqs.is_empty() {
            return None;
        }
        Some(Expr::and(eqs))
    }

    pub fn cond(&mut self, c: &Cond) -> Option<Expr> {
        let b = &self.prog.builtins;
        Some(match c {
            Cond::Con(v, con, fields) => {
                if Some(*con) == b.true_ {
                    self.boolean(v, 0)
                } else if Some(*con) == b.false_ {
                    Expr::negation(self.boolean(v, 0))
                } else if Some(*con) == b.zero {
                    Expr::eq(self.int(v, 0), Expr::Int(0))
                } else if Some(*con) == b.succ && fields.len() == 1 {
                    let f = self.int(&Val::var(fields[0]), 0);
                    Expr::eq(self.int(v, 0), Expr::plus(f, Expr::Int(1)))
                } else {
                    return self.record_cond(v, *con, fields);
                }
            }
            Cond::Lit(v, Lit::Int(n)) => Expr::eq(self.int(v, 0), Expr::Int(*n as i128)),
            Cond::NotLits(v, ls) => {
                let s = self.int(v, 0);
                Expr::and(
                    ls.iter()
                        .filter_map(|l| match l {
                            Lit::Int(n) => {
                                Some(Expr::negation(Expr::eq(s.clone(), Expr::Int(*n as i128))))
                            }
                            _ => None,
                        })
                        .collect(),
                )
            }
            Cond::NotCons(v, cs) => {
                let only = |g: Option<GName>| cs.len() == 1 && Some(cs[0]) == g;
                if only(b.true_) {
                    Expr::negation(self.boolean(v, 0))
                } else if only(b.false_) {
                    self.boolean(v, 0)
                } else if only(b.zero) {
                    Expr::le(Expr::Int(1), self.int(v, 0))
                } else if only(b.succ) {
                    Expr::eq(self.int(v, 0), Expr::Int(0))
                } else {
                    return None;
                }
            }
            Cond::Lit(..) => return None,
        })
    }
}

/// Checks the obligations of `m` at every self-call.
pub fn check(ev: &Ev, solver: &Solver, facts: &Facts, m: &Measure) -> Result<(), Failure> {
    let names = &smt_names(facts);
    let np = match m {
        Measure::Given(g) => g.nparams,
        Measure::Param(i) => i + 1,
    };
    if facts.nparams < np {
        return Err(Failure::Partial);
    }
    let params: Vec<Val> = (0..np as u32).map(Val::var).collect();
    let nat = match m {
        Measure::Given(g) => g.nat,
        Measure::Param(i) => facts.sorts.get(&(*i as u32)) == Some(&Sort::Nat),
    };
    for SelfCall { args, conds } in &facts.self_calls {
        if args.len() < np {
            return Err(Failure::Partial);
        }
        let mut tr = Tr::new(ev, &facts.sorts, names);
        let (old, new) = match m {
            Measure::Given(g) => {
                let o = ev.eval(&params, &g.term);
                let n = ev.eval(&args[..np].to_vec(), &g.term);
                (tr.int(&o, 0), tr.int(&n, 0))
            }
            Measure::Param(i) => (tr.int(&params[*i], 0), tr.int(&args[*i], 0)),
        };
        let mut hyps: Vec<Expr> = conds.iter().filter_map(|c| tr.cond(c)).collect();
        let mut goal = vec![Expr::lt(new, old.clone())];
        if !nat {
            goal.push(Expr::le(Expr::Int(0), old));
        }
        hyps.extend(tr.var_hyps());
        let mut q = Query {
            hyps,
            goal: Some(Expr::and(goal)),
        };
        tr.bound_ints(&mut q);
        match solver.prove(&q) {
            Ok(Outcome::Valid) => {}
            Ok(Outcome::Invalid(model)) => {
                let model = model
                    .into_iter()
                    .filter(|(k, _)| !k.starts_with("t$"))
                    .collect();
                return Err(Failure::Counterexample(model));
            }
            Ok(Outcome::Unknown(why)) => return Err(Failure::Unknown(why)),
            Err(e) => return Err(Failure::Unknown(e)),
        }
    }
    Ok(())
}

/// The term without its refinement marks (`primRefine {a} p site e` is
/// `e`), so that the same value checked at different places is the same
/// SMT term.
pub(crate) fn strip_refine(t: &Term, refine: Option<GName>) -> Term {
    let go = |t: &Term| std::rc::Rc::new(strip_refine(t, refine));
    match t {
        Term::App(f, a, i) => {
            if let Term::App(f2, _, _) = f.as_ref()
                && let Term::App(f3, _, _) = f2.as_ref()
                && let Term::App(h, _, _) = f3.as_ref()
                && matches!(h.as_ref(), Term::Global(g) if Some(*g) == refine)
            {
                return strip_refine(a, refine);
            }
            Term::App(go(f), go(a), *i)
        }
        Term::Lam(x, m, i, ty, b) => Term::Lam(x.clone(), *m, *i, ty.clone(), go(b)),
        Term::Let(x, m, ty, v, b) => Term::Let(x.clone(), *m, ty.clone(), go(v), go(b)),
        Term::Case(c) => {
            let mut c2 = c.as_ref().clone();
            c2.scrut = strip_refine(&c.scrut, refine);
            for a in &mut c2.alts {
                match a {
                    Alt::Con { body, .. } | Alt::Lit { body, .. } => {
                        *body = strip_refine(body, refine)
                    }
                }
            }
            c2.default = c.default.as_ref().map(|d| strip_refine(d, refine));
            Term::Case(std::rc::Rc::new(c2))
        }
        _ => t.clone(),
    }
}

/// The levels of the variables that stand for refined results.
const STAND_IN_LEVEL: u32 = 90_000;

/// Field `i` of `n` of a value of a one-constructor type, as a stuck match
/// on it (the same value wherever it is built, so the same SMT variable).
fn projection(scrut: &Val, con: GName, n: usize, i: usize) -> Val {
    let proj = Rc::new(Case {
        scrut: Term::Var(0),
        ret: Term::Error,
        alts: vec![Alt::Con {
            con,
            fields: (0..n)
                .map(|_| (Name::from("_"), Mult::Many, Term::Error))
                .collect(),
            refine: Vec::new(),
            body: Term::Var((n - 1 - i) as u32),
        }],
        default: None,
    });
    match scrut {
        Val::Neutral(h, sp) => {
            let mut sp2 = sp.clone();
            sp2.push(Elim::Case(Vec::new(), proj));
            Val::Neutral(h.clone(), sp2)
        }
        v => v.clone(),
    }
}
