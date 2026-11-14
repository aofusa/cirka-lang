//! The facts about one definition that totality needs (spec-v0.0.4.md
//! §1): the globals it uses, its calls with the size relation of each
//! argument to its parameters (for the size-change principle), and its
//! self-calls with their path conditions (for measures).
//!
//! The body is walked with an environment of values (NbE), so that calls
//! through trait methods of known instances are followed to the method
//! (`x == y` at `List a` calls the `==` of the instance for lists), and
//! functions that receive a known instance (default methods) are walked
//! inline with it.

use cirkalang_elab::eval::Ev;
use cirkalang_elab::tt::*;
use std::collections::{BTreeSet, HashMap};

/// How an argument relates to a parameter of the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rel {
    /// Equal (the parameter itself).
    Eq,
    /// Strictly smaller (a part of the parameter's value).
    Lt,
}

/// The sort of a variable for the SMT translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Int,
    Nat,
    Bool,
}

/// What a path through the body has learnt about a value.
#[derive(Clone, Debug)]
pub enum Cond {
    /// The value matched a constructor; its fields are these variables.
    Con(Val, GName, Vec<u32>),
    Lit(Val, Lit),
    /// The value matched none of these constructors / literals.
    NotCons(Val, Vec<GName>),
    NotLits(Val, Vec<Lit>),
}

/// A call: for each argument, its relations to the caller's parameters.
#[derive(Clone, Debug)]
pub struct Call {
    pub callee: GName,
    pub rels: Vec<Vec<(usize, Rel)>>,
}

/// A call of the definition to itself, for measures.
#[derive(Clone, Debug)]
pub struct SelfCall {
    pub args: Vec<Val>,
    pub conds: Vec<Cond>,
}

/// A value checked against a refinement type (spec-v0.0.4.md §3): `pred
/// val` must hold under `conds` and the refinements of the binders in
/// scope.
#[derive(Clone, Debug)]
pub struct Obligation {
    pub site: usize,
    pub pred: Val,
    pub val: Val,
    pub conds: Vec<Cond>,
    /// What the binders in scope tell.
    pub assumed: Vec<Assume>,
}

/// What a binder in scope tells about the values.
#[derive(Clone, Debug)]
pub enum Assume {
    /// A variable (or `let`) of a refinement type: this boolean (the
    /// predicate applied to the value) holds.
    Holds(Val),
    /// A proof of `So e` or `x = y` (its type): a manual proof
    /// (spec-v0.0.4.md §3.4).
    Fact(Val),
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// Number of leading lambdas (the parameters).
    pub nparams: usize,
    pub deps: BTreeSet<GName>,
    pub calls: Vec<Call>,
    pub self_calls: Vec<SelfCall>,
    /// Sorts of the variables (by level) that the SMT translation knows.
    pub sorts: HashMap<u32, Sort>,
    /// Source names of the variables (by level).
    pub names: HashMap<u32, String>,
    pub obligations: Vec<Obligation>,
}

/// The refinements of the result of `g args` (several when nested).
pub fn result_refinements(prog: &Program, ev: &Ev, g: GName, sp: &[Elim]) -> Vec<Val> {
    let ty = match prog.get(g) {
        Global::Def { ty, .. } | Global::Con { ty, .. } | Global::Extern { ty, .. } => ty,
        _ => return Vec::new(),
    };
    let mut t = ev.eval(&Vec::new(), ty);
    for e in sp {
        let Elim::App(a, _) = e else {
            return Vec::new();
        };
        ev.fuel.set(10_000);
        match ev.whnf(t) {
            Val::Pi(_, _, _, _, c) => t = ev.inst(&c, a.clone()),
            _ => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    while let Some((a, q)) = subset_parts(prog, ev, &t) {
        out.push(q);
        t = a;
    }
    out
}

/// `subsetOf a p` (not reduced): `(a, p)`.
pub fn subset_parts(prog: &Program, ev: &Ev, ty: &Val) -> Option<(Val, Val)> {
    let Val::Neutral(Head::Global(g), sp) = ev.force(ty.clone()) else {
        return None;
    };
    if Some(g) != prog.builtins.subset_of || sp.len() != 2 {
        return None;
    }
    match (&sp[0], &sp[1]) {
        (Elim::App(a, _), Elim::App(p, _)) => Some((a.clone(), p.clone())),
        _ => None,
    }
}

/// A trait method or supertrait selector: `\{params} => case d of
/// Dict fs => f_k`.
#[derive(Clone, Copy, Debug)]
pub struct Selector {
    pub nparams: usize,
    /// Position of the dictionary among the parameters.
    pub dict: usize,
    /// Index of the selected field.
    pub field: usize,
}

/// Finds the selectors of the program.
pub fn selectors(prog: &Program) -> HashMap<GName, Selector> {
    let mut out = HashMap::new();
    for (i, g) in prog.globals.iter().enumerate() {
        let Global::Def { body: Some(b), .. } = g else {
            continue;
        };
        let mut t = b;
        let mut n = 0usize;
        while let Term::Lam(_, _, _, _, body) = t {
            t = body;
            n += 1;
        }
        let Term::Case(c) = t else { continue };
        let (Term::Var(s), [Alt::Con { fields, body, .. }], None) =
            (&c.scrut, c.alts.as_slice(), &c.default)
        else {
            continue;
        };
        let Term::Var(j) = body else { continue };
        let (s, j) = (*s as usize, *j as usize);
        if s < n && j < fields.len() {
            out.insert(
                GName(i as u32),
                Selector {
                    nparams: n,
                    dict: n - 1 - s,
                    field: fields.len() - 1 - j,
                },
            );
        }
    }
    out
}

pub struct Walker<'a> {
    pub ev: &'a Ev<'a>,
    prog: &'a Program,
    sels: &'a HashMap<GName, Selector>,
    dict_cons: &'a BTreeSet<GName>,
    me: GName,
    env: Vec<Val>,
    next: u32,
    /// Relations of variables (by level) to the parameters.
    sizes: HashMap<u32, Vec<(usize, Rel)>>,
    conds: Vec<Cond>,
    inlining: Vec<GName>,
    /// What the binders in scope tell.
    assumed: Vec<Assume>,
    pub facts: Facts,
}

const MAX_INLINE: usize = 3;

impl<'a> Walker<'a> {
    pub fn new(
        ev: &'a Ev<'a>,
        sels: &'a HashMap<GName, Selector>,
        dict_cons: &'a BTreeSet<GName>,
        me: GName,
    ) -> Walker<'a> {
        Walker {
            ev,
            prog: ev.prog,
            sels,
            dict_cons,
            me,
            env: Vec::new(),
            next: 0,
            sizes: HashMap::new(),
            conds: Vec::new(),
            inlining: Vec::new(),
            assumed: Vec::new(),
            facts: Facts::default(),
        }
    }

    /// Walks the body of the definition: its leading lambdas are the
    /// parameters (levels `0..nparams`).
    pub fn run(&mut self, body: &Term) {
        let mut t = body;
        while let Term::Lam(x, _, _, ty, b) = t {
            let l = self.fresh(x, ty);
            let i = self.facts.nparams;
            self.sizes.insert(l, vec![(i, Rel::Eq)]);
            self.facts.nparams += 1;
            self.env.push(Val::var(l));
            t = b;
        }
        self.walk(t);
    }

    fn fresh(&mut self, name: &str, ty: &Term) -> u32 {
        let l = self.next;
        self.next += 1;
        self.facts.names.insert(l, name.to_string());
        let tv = self.ev.eval(&self.env, ty);
        // `{x : {x : A | p} | q}` (several contracts): both hold.
        let mut t = tv.clone();
        while let Some((a, p)) = subset_parts(self.prog, self.ev, &t) {
            let holds = self.ev.apply(p, Val::var(l), Icit::Expl);
            self.assumed.push(Assume::Holds(holds));
            t = a;
        }
        let tv = self.whnf(tv);
        self.fact(&tv);
        if let Some(s) = self.sort_of(&tv) {
            self.facts.sorts.insert(l, s);
        }
        l
    }

    /// A binder whose type is a proof of `So e` or `x = y`.
    fn fact(&mut self, ty: &Val) {
        if let Val::Data(g, _) = ty
            && (Some(*g) == self.prog.builtins.equal || self.prog.name(*g) == "Prelude.So")
        {
            self.assumed.push(Assume::Fact(ty.clone()));
        }
    }

    fn sort_of(&self, ty: &Val) -> Option<Sort> {
        let b = &self.prog.builtins;
        match ty {
            Val::Neutral(Head::Global(g), sp) if sp.is_empty() && Some(*g) == b.int => {
                Some(Sort::Int)
            }
            Val::Data(g, ps) if ps.is_empty() && Some(*g) == b.nat => Some(Sort::Nat),
            Val::Data(g, ps) if ps.is_empty() && Some(*g) == b.bool_ => Some(Sort::Bool),
            _ => None,
        }
    }

    fn eval(&self, t: &Term) -> Val {
        self.ev.eval(&self.env, t)
    }

    fn walk(&mut self, t: &Term) {
        match t {
            Term::Var(_) => {
                let v = self.eval(t);
                self.val_deps(&v);
            }
            Term::Global(g) => self.call(*g, Vec::new()),
            Term::App(..) => {
                let mut args = Vec::new();
                let mut h = t;
                while let Term::App(f, a, _) = h {
                    args.push(a.as_ref());
                    h = f;
                }
                args.reverse();
                match h {
                    Term::Global(g) => self.app_global(*g, &args),
                    Term::Var(_) => {
                        // A variable bound by inlining to a global.
                        let hv = self.eval(h);
                        for a in &args {
                            self.walk(a);
                        }
                        match hv {
                            Val::Neutral(Head::Global(g), sp)
                                if sp.iter().all(|e| matches!(e, Elim::App(..))) =>
                            {
                                let mut vals: Vec<Val> = sp
                                    .into_iter()
                                    .filter_map(|e| match e {
                                        Elim::App(v, _) => Some(v),
                                        Elim::Case(..) => None,
                                    })
                                    .collect();
                                vals.extend(args.iter().map(|a| self.eval(a)));
                                self.call(g, vals);
                            }
                            other => self.val_deps(&other),
                        }
                    }
                    _ => {
                        self.walk(h);
                        for a in &args {
                            self.walk(a);
                        }
                    }
                }
            }
            Term::Lam(x, _, _, ty, b) => {
                let n = self.assumed.len();
                let l = self.fresh(x, ty);
                self.env.push(Val::var(l));
                self.walk(b);
                self.env.pop();
                self.assumed.truncate(n);
            }
            Term::Let(_, _, ty, v, b) => {
                self.walk(v);
                let vv = self.eval(v);
                let n = self.assumed.len();
                let tv = self.eval(ty);
                // A refined value: by its type, or as the result of a
                // function with a refined result.
                let mut refined = Vec::new();
                let mut t = tv.clone();
                while let Some((a, p)) = subset_parts(self.prog, self.ev, &t) {
                    refined.push(p);
                    t = a;
                }
                // Only from a total function: an unused `let` may be removed,
                // and the fact must not come from a call that never returns.
                if refined.is_empty()
                    && let Val::Neutral(Head::Global(g), sp) = &vv
                    && matches!(self.prog.totality.get(g), Some((Totality::Total, _)))
                {
                    refined = result_refinements(self.prog, self.ev, *g, sp);
                }
                for p in refined {
                    let holds = self.ev.apply(p, vv.clone(), Icit::Expl);
                    self.assumed.push(Assume::Holds(holds));
                }
                let tv = self.whnf(tv);
                self.fact(&tv);
                self.env.push(vv);
                self.walk(b);
                self.env.pop();
                self.assumed.truncate(n);
            }
            Term::Case(c) => self.case(c),
            Term::Pi(..)
            | Term::Type
            | Term::LType
            | Term::Lit(_)
            | Term::Meta(_)
            | Term::InsertedMeta(..)
            | Term::Error => {}
        }
    }

    fn case(&mut self, c: &Case) {
        self.walk(&c.scrut);
        let sv = self.eval(&c.scrut);
        let smaller: Vec<(usize, Rel)> = match &sv {
            Val::Neutral(Head::Var(l), sp) if sp.is_empty() => self
                .sizes
                .get(l)
                .map(|r| r.iter().map(|(i, _)| (*i, Rel::Lt)).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let mut cons = Vec::new();
        let mut lits = Vec::new();
        for a in &c.alts {
            match a {
                Alt::Con {
                    con, fields, body, ..
                } => {
                    cons.push(*con);
                    self.call(*con, Vec::new());
                    let n = self.env.len();
                    let na = self.assumed.len();
                    let mut ls = Vec::new();
                    for (x, _, ty) in fields {
                        let l = self.fresh(x, ty);
                        if !smaller.is_empty() {
                            self.sizes.insert(l, smaller.clone());
                        }
                        ls.push(l);
                        self.env.push(Val::var(l));
                    }
                    self.conds.push(Cond::Con(sv.clone(), *con, ls));
                    self.walk(body);
                    self.conds.pop();
                    self.env.truncate(n);
                    self.assumed.truncate(na);
                }
                Alt::Lit { lit, body } => {
                    lits.push(lit.clone());
                    self.conds.push(Cond::Lit(sv.clone(), lit.clone()));
                    self.walk(body);
                    self.conds.pop();
                }
            }
        }
        if let Some(d) = &c.default {
            let cond = if lits.is_empty() {
                Cond::NotCons(sv.clone(), cons)
            } else {
                Cond::NotLits(sv.clone(), lits)
            };
            self.conds.push(cond);
            self.walk(d);
            self.conds.pop();
        }
    }

    /// `g args`.
    fn app_global(&mut self, g: GName, args: &[&Term]) {
        // A value checked against a refinement type.
        if Some(g) == self.prog.builtins.refine
            && args.len() == 4
            && self.inlining.is_empty()
            && let Term::Lit(Lit::Int(site)) = args[2]
        {
            let ob = Obligation {
                site: *site as usize,
                pred: self.eval(args[1]),
                val: self.eval(args[3]),
                conds: self.conds.clone(),
                assumed: self.assumed.clone(),
            };
            self.facts.obligations.push(ob);
        }
        // A trait method of a known instance: call the method.
        if let Some(sel) = self.sels.get(&g).copied()
            && args.len() > sel.dict
            && let Some(field) = self.field(sel, args[sel.dict])
        {
            let rest = &args[sel.nparams.min(args.len())..];
            let mut f = field;
            for a in rest {
                let v = self.eval(a);
                f = self.ev.apply(f, v, Icit::Expl);
            }
            if let Val::Neutral(Head::Global(m), sp) = f
                && sp.iter().all(|e| matches!(e, Elim::App(..)))
                && matches!(self.prog.get(m), Global::Def { .. })
            {
                self.walk_dict_args(args[sel.dict]);
                for a in rest {
                    self.walk(a);
                }
                let vals = sp
                    .into_iter()
                    .filter_map(|e| match e {
                        Elim::App(v, _) => Some(v),
                        Elim::Case(..) => None,
                    })
                    .collect();
                self.call(m, vals);
                return;
            }
        }
        // A function given a known instance: walk it with the instance.
        if let Global::Def {
            body: Some(body), ..
        } = self.prog.get(g)
            && g != self.me
            && !self.sels.contains_key(&g)
            && self.inlining.len() < MAX_INLINE
            && !self.inlining.contains(&g)
        {
            let vals: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
            let dicts: Vec<bool> = vals.iter().map(|v| self.is_dict(v)).collect();
            if dicts.iter().any(|d| *d) {
                for (a, d) in args.iter().zip(&dicts) {
                    if *d {
                        self.walk_dict_args(a);
                    } else {
                        self.walk(a);
                    }
                }
                self.inline(g, body, vals);
                return;
            }
        }
        for a in args {
            self.walk(a);
        }
        let vals = args.iter().map(|a| self.eval(a)).collect();
        self.call(g, vals);
    }

    /// The field selected from the dictionary `d` (a known instance).
    fn field(&self, sel: Selector, d: &Term) -> Option<Val> {
        let dv = self.whnf(self.eval(d));
        let Val::Con(c, vals) = dv else {
            return None;
        };
        let np = match self.prog.get(c) {
            Global::Con { nparams, .. } => *nparams,
            _ => return None,
        };
        vals.get(np + sel.field).cloned()
    }

    fn whnf(&self, v: Val) -> Val {
        self.ev.fuel.set(10_000);
        self.ev.whnf(v)
    }

    fn is_dict(&self, v: &Val) -> bool {
        let head_global = match v {
            Val::Neutral(Head::Global(g), _) => Some(*g),
            _ => None,
        };
        let Some(g) = head_global else {
            return false;
        };
        if !matches!(self.prog.get(g), Global::Def { .. }) {
            return false;
        }
        matches!(self.whnf(v.clone()), Val::Con(c, _) if self.dict_cons.contains(&c))
    }

    /// The arguments of an instance (its context dictionaries), without the
    /// instance itself.
    fn walk_dict_args(&mut self, d: &Term) {
        let mut h = d;
        while let Term::App(f, a, _) = h {
            self.walk(a);
            h = f;
        }
        if !matches!(h, Term::Global(_)) {
            self.walk(h);
        }
    }

    fn inline(&mut self, g: GName, body: &Term, vals: Vec<Val>) {
        let env = std::mem::take(&mut self.env);
        self.inlining.push(g);
        let mut t = body;
        let mut vals = vals.into_iter();
        while let Term::Lam(_, _, _, _, b) = t
            && let Some(v) = vals.next()
        {
            self.env.push(v);
            t = b;
        }
        self.walk(t);
        self.inlining.pop();
        self.env = env;
    }

    /// The globals a value refers to (a variable bound by inlining).
    fn val_deps(&mut self, v: &Val) {
        match v {
            Val::Neutral(h, sp) => {
                if let Head::Global(g) = h {
                    self.facts.deps.insert(*g);
                }
                for e in sp {
                    if let Elim::App(a, _) = e {
                        self.val_deps(a);
                    }
                }
            }
            Val::Con(c, args) => {
                self.facts.deps.insert(*c);
                for a in args {
                    self.val_deps(a);
                }
            }
            _ => {}
        }
    }

    /// The relations of a value to the parameters: a variable, or a
    /// constructor applied to the fields of a value it was matched with
    /// (`S m` where `x` matched `S m` is `x`).
    fn rel_of(&self, v: &Val) -> Vec<(usize, Rel)> {
        match v {
            Val::Neutral(Head::Var(l), sp) if sp.is_empty() => {
                self.sizes.get(l).cloned().unwrap_or_default()
            }
            Val::Con(c, args) => {
                let np = match self.prog.get(*c) {
                    Global::Con { nparams, .. } => *nparams,
                    _ => return Vec::new(),
                };
                let fields: Vec<Option<u32>> = args
                    .iter()
                    .skip(np)
                    .map(|a| match a {
                        Val::Neutral(Head::Var(l), sp) if sp.is_empty() => Some(*l),
                        _ => None,
                    })
                    .collect();
                for cond in self.conds.iter().rev() {
                    if let Cond::Con(sv, c2, ls) = cond
                        && c2 == c
                        && ls.len() == fields.len()
                        && ls.iter().zip(&fields).all(|(l, f)| Some(*l) == *f)
                    {
                        return self.rel_of(sv);
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn call(&mut self, g: GName, args: Vec<Val>) {
        self.facts.deps.insert(g);
        if !matches!(self.prog.get(g), Global::Def { .. }) {
            return;
        }
        let rels = args.iter().map(|a| self.rel_of(a)).collect();
        self.facts.calls.push(Call { callee: g, rels });
        if g == self.me {
            self.facts.self_calls.push(SelfCall {
                args,
                conds: self.conds.clone(),
            });
        }
    }
}
