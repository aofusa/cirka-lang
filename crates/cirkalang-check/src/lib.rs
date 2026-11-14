//! cirkalang-check: checks on the elaborated program that are not part of
//! type checking (13-verification.md).
//!
//! Totality (spec-v0.0.4.md §1): a function is total when its patterns are
//! exhaustive (checked by elaboration unless it is `partial`), it
//! terminates and it only uses total functions. Termination is shown by
//! the size-change principle over the structural order of values, or by a
//! measure decided by Z3 (`decreasing e`, or an `Int` / `Nat` parameter
//! tried automatically). Functions declared `total` and functions used in
//! types must be total.

mod measure;
mod refine;
mod walk;

use cirka_diagnostics::{Diagnostic, Span};
use cirka_smt::Solver;
use cirkalang_elab::eval::{Ev, Metas};
use cirkalang_elab::tt::*;
use cirkalang_resolve::rast;
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use walk::{Facts, Rel, Sort, Walker};

/// Why a definition is not total.
#[derive(Clone, Debug)]
enum Why {
    /// Declared `partial`.
    Partial,
    /// Uses `panic`.
    Panic,
    /// Its definition is not available.
    NoBody,
    /// Termination could not be shown (with what the measure found).
    Loops(Option<String>),
    /// Uses a definition that is not total.
    Calls(GName),
    /// Uses a data type that is not strictly positive.
    NonPositive(GName),
}

/// What to do with refinements and contracts (`--contracts`,
/// spec-v0.0.4.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Contracts {
    /// Prove them; what cannot be proved is an error.
    #[default]
    Verify,
    /// Prove them; what cannot be proved is checked at run time.
    Check,
    /// Ignore them.
    Off,
}

impl Contracts {
    pub fn parse(s: &str) -> Option<Contracts> {
        Some(match s {
            "verify" => Contracts::Verify,
            "check" => Contracts::Check,
            "off" => Contracts::Off,
            _ => return None,
        })
    }
}

/// The result of the checks.
#[derive(Debug, Default)]
pub struct Checked {
    /// Errors and warnings.
    pub diags: Vec<Diagnostic>,
    /// Refinement sites that were not proved (`--contracts=check`), with the
    /// message to panic with when they fail at run time.
    pub unproved: Vec<(usize, String)>,
}

/// Checks totality and refinements (`--contracts=verify`). Returns
/// diagnostics. Decided SMT queries are remembered in `cache` (a
/// directory) when given.
pub fn check(rp: &rast::Program, prog: &Program, cache: Option<PathBuf>) -> Vec<Diagnostic> {
    check_with(rp, prog, cache, Contracts::Verify).diags
}

/// Replaces the refinement marks of `sites` by run-time checks:
/// `primRefine {a} p site e` becomes `checkContract {a} p msg e`
/// (`--contracts=check`).
pub fn insert_checks(prog: &mut Program, sites: &[(usize, String)]) {
    let (Some(refine), Some(bool_ty), Some(chk)) = (
        prog.builtins.refine,
        prog.builtins.bool_,
        prog.names
            .iter()
            .position(|n| n == "Prelude.checkContract")
            .map(|i| GName(i as u32)),
    ) else {
        return;
    };
    let msgs: HashMap<usize, String> = sites.iter().cloned().collect();
    for g in prog.globals.iter_mut() {
        if let Global::Def { body: Some(b), .. } = g {
            *b = refine::insert(b, refine, chk, bool_ty, &msgs);
        }
    }
}

/// Checks totality and refinements.
pub fn check_with(
    rp: &rast::Program,
    prog: &Program,
    cache: Option<PathBuf>,
    contracts: Contracts,
) -> Checked {
    let mut wanted: Vec<GName> = prog
        .totality
        .iter()
        .filter(|(_, (t, _))| *t == Totality::Total)
        .map(|(g, _)| *g)
        .collect();
    wanted.sort();
    let in_types = used_in_types(prog);
    let proofs = proofs(prog);
    let refined = !prog.refine_sites.is_empty() && contracts != Contracts::Off;
    if wanted.is_empty() && in_types.is_empty() && proofs.is_empty() && !refined {
        return Checked::default();
    }
    let metas = Metas::default();
    let fuel = Cell::new(10_000);
    let oof = Cell::new(false);
    let ev = Ev::new(prog, &metas, &fuel, &oof);
    let mut t = Totality_ {
        prog,
        ev: &ev,
        sels: walk::selectors(prog),
        dict_cons: prog.traits.values().map(|t| t.con).collect(),
        facts: HashMap::new(),
        status: HashMap::new(),
        positive: HashMap::new(),
        solver: Solver::from_env().map(|s| match cache {
            Some(d) => s.with_cache(d),
            None => s,
        }),
    };
    let mut diags = Vec::new();
    let mut unproved = Vec::new();
    if refined {
        t.refinements(contracts, &mut diags, &mut unproved);
    }
    let roots: Vec<GName> = wanted
        .iter()
        .copied()
        .chain(in_types.iter().map(|(_, f)| *f))
        .chain(proofs.iter().copied())
        .collect();
    t.explore(&roots);
    t.decide();
    let span = |g: GName| rp.info(g).span;
    for f in &wanted {
        if let Some(why) = t.status.get(f).cloned().flatten() {
            diags.push(t.not_total(*f, &why, span(*f)));
        }
    }
    for f in &proofs {
        if wanted.contains(f) {
            continue;
        }
        if let Some(why) = t.status.get(f).cloned().flatten() {
            diags.push(
                Diagnostic::error(format!("the proof `{}` is not total", prog.short_name(*f)))
                    .with_code("E0506")
                    .with_label(span(*f), "a proof (its result is an equality or `Void`)")
                    .with_note(t.explain(*f, &why))
                    .with_help("a proof must terminate on every argument (spec-v0.0.4.md §2.4)"),
            );
        }
    }
    let mut seen = HashSet::new();
    for (user, f) in &in_types {
        if !seen.insert((*user, *f)) {
            continue;
        }
        if let Some(why) = t.status.get(f).cloned().flatten() {
            diags.push(
                Diagnostic::error(format!(
                    "`{}` is used in a type but is not total",
                    prog.short_name(*f)
                ))
                .with_code("E0505")
                .with_label(
                    span(*user),
                    format!("the type of `{}`", prog.short_name(*user)),
                )
                .with_note(t.explain(*f, &why))
                .with_help("functions in types must be total (spec-v0.0.4.md §1.4)"),
            );
        }
    }
    Checked { diags, unproved }
}

/// Definitions whose result is a proof: an equality or `Void`.
fn proofs(prog: &Program) -> Vec<GName> {
    let b = &prog.builtins;
    let mut out = Vec::new();
    for (i, g) in prog.globals.iter().enumerate() {
        let Global::Def { ty, body: Some(_) } = g else {
            continue;
        };
        let mut t = ty;
        while let Term::Pi(_, _, _, _, c) = t {
            t = c;
        }
        while let Term::App(f, _, _) = t {
            t = f;
        }
        if let Term::Global(h) = t
            && (Some(*h) == b.equal || Some(*h) == b.void)
        {
            out.push(GName(i as u32));
        }
    }
    out
}

/// (user, function): definitions used in the types of globals.
fn used_in_types(prog: &Program) -> Vec<(GName, GName)> {
    fn globals(t: &Term, out: &mut Vec<GName>) {
        match t {
            Term::Global(g) => out.push(*g),
            Term::Pi(_, _, _, a, b) | Term::Lam(_, _, _, a, b) | Term::App(a, b, _) => {
                globals(a, out);
                globals(b, out);
            }
            Term::Let(_, _, a, b, c) => {
                globals(a, out);
                globals(b, out);
                globals(c, out);
            }
            Term::Case(c) => {
                globals(&c.scrut, out);
                for a in &c.alts {
                    match a {
                        Alt::Con { body, .. } | Alt::Lit { body, .. } => globals(body, out),
                    }
                }
                if let Some(d) = &c.default {
                    globals(d, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (i, g) in prog.globals.iter().enumerate() {
        let ty = match g {
            Global::Def { ty, .. }
            | Global::Data { ty, .. }
            | Global::Con { ty, .. }
            | Global::Prim { ty, .. }
            | Global::Extern { ty, .. } => ty,
            Global::Missing => continue,
        };
        let mut gs = Vec::new();
        globals(ty, &mut gs);
        for f in gs {
            if matches!(prog.get(f), Global::Def { .. }) {
                out.push((GName(i as u32), f));
            }
        }
    }
    out
}

struct Totality_<'a> {
    prog: &'a Program,
    ev: &'a Ev<'a>,
    sels: HashMap<GName, walk::Selector>,
    dict_cons: BTreeSet<GName>,
    facts: HashMap<GName, Facts>,
    /// `None` when total.
    status: HashMap<GName, Option<Why>>,
    positive: HashMap<GName, bool>,
    solver: Option<Solver>,
}

impl Totality_<'_> {
    /// Walks the definitions reachable from `roots`.
    /// Checks the refinement obligations of every definition that has
    /// some (spec-v0.0.4.md §3).
    fn refinements(
        &mut self,
        mode: Contracts,
        diags: &mut Vec<Diagnostic>,
        unproved: &mut Vec<(usize, String)>,
    ) {
        let Some(refine) = self.prog.builtins.refine else {
            return;
        };
        for g in self.prog.order.clone() {
            let Global::Def { body: Some(b), .. } = self.prog.get(g) else {
                continue;
            };
            if !refine::mentions_refine(b, refine) {
                continue;
            }
            let mut w = Walker::new(self.ev, &self.sels, &self.dict_cons, g);
            w.run(b);
            refine::check(
                self.ev,
                self.solver.as_ref(),
                self.prog,
                &w.facts,
                mode,
                diags,
                unproved,
            );
        }
    }

    fn explore(&mut self, roots: &[GName]) {
        let mut work: Vec<GName> = roots.to_vec();
        while let Some(g) = work.pop() {
            if self.facts.contains_key(&g) {
                continue;
            }
            let Global::Def { body: Some(b), .. } = self.prog.get(g) else {
                continue;
            };
            let mut w = Walker::new(self.ev, &self.sels, &self.dict_cons, g);
            w.run(b);
            let facts = w.facts;
            for d in &facts.deps {
                if matches!(self.prog.get(*d), Global::Def { body: Some(_), .. })
                    && !self.facts.contains_key(d)
                {
                    work.push(*d);
                }
            }
            self.facts.insert(g, facts);
        }
    }

    /// Decides every explored definition, callees first.
    fn decide(&mut self) {
        for scc in self.sccs() {
            let why = self.decide_scc(&scc);
            for f in scc {
                let w = match (&why, self.prog.totality.get(&f)) {
                    (_, Some((Totality::Partial, _))) => Some(Why::Partial),
                    (w, _) => w.clone(),
                };
                self.status.insert(f, w);
            }
        }
    }

    fn decide_scc(&mut self, scc: &[GName]) -> Option<Why> {
        let members: HashSet<GName> = scc.iter().copied().collect();
        if scc
            .iter()
            .any(|f| matches!(self.prog.totality.get(f), Some((Totality::Partial, _))))
        {
            return Some(Why::Partial);
        }
        for f in scc {
            let deps: Vec<GName> = self.facts[f].deps.iter().copied().collect();
            for d in deps {
                if members.contains(&d) {
                    continue;
                }
                if let Some(w) = self.dep_why(d) {
                    return Some(w);
                }
            }
        }
        if self.size_change(scc) {
            return None;
        }
        if let [f] = scc {
            return match self.by_measure(*f) {
                Ok(()) => None,
                Err(why) => Some(Why::Loops(why)),
            };
        }
        Some(Why::Loops(None))
    }

    fn dep_why(&mut self, d: GName) -> Option<Why> {
        match self.prog.get(d) {
            Global::Def { body: Some(_), .. } => match self.status.get(&d) {
                Some(None) => None,
                Some(Some(_)) => Some(Why::Calls(d)),
                None => Some(Why::NoBody),
            },
            Global::Def { body: None, .. } | Global::Missing => Some(Why::Calls(d)),
            Global::Prim {
                prim: Prim::Panic, ..
            } => Some(Why::Panic),
            Global::Con { data, .. } => {
                let data = *data;
                (!self.is_positive(data)).then_some(Why::NonPositive(data))
            }
            _ => None,
        }
    }

    /// Tarjan's algorithm over the explored definitions; callees first.
    fn sccs(&self) -> Vec<Vec<GName>> {
        struct St {
            index: HashMap<GName, usize>,
            low: HashMap<GName, usize>,
            stack: Vec<GName>,
            on: HashSet<GName>,
            out: Vec<Vec<GName>>,
            next: usize,
        }
        let mut st = St {
            index: HashMap::new(),
            low: HashMap::new(),
            stack: Vec::new(),
            on: HashSet::new(),
            out: Vec::new(),
            next: 0,
        };
        let mut nodes: Vec<GName> = self.facts.keys().copied().collect();
        nodes.sort();
        // Iterative DFS: (node, next dependency index).
        for &root in &nodes {
            if st.index.contains_key(&root) {
                continue;
            }
            let mut frames: Vec<(GName, Vec<GName>, usize)> = Vec::new();
            let succ = |g: GName| -> Vec<GName> {
                self.facts[&g]
                    .deps
                    .iter()
                    .copied()
                    .filter(|d| self.facts.contains_key(d))
                    .collect()
            };
            st.index.insert(root, st.next);
            st.low.insert(root, st.next);
            st.next += 1;
            st.stack.push(root);
            st.on.insert(root);
            frames.push((root, succ(root), 0));
            while let Some((v, ss, i)) = frames.last_mut() {
                let v = *v;
                if *i < ss.len() {
                    let w = ss[*i];
                    *i += 1;
                    if !st.index.contains_key(&w) {
                        st.index.insert(w, st.next);
                        st.low.insert(w, st.next);
                        st.next += 1;
                        st.stack.push(w);
                        st.on.insert(w);
                        frames.push((w, succ(w), 0));
                    } else if st.on.contains(&w) {
                        let lw = st.index[&w];
                        let lv = st.low[&v];
                        st.low.insert(v, lv.min(lw));
                    }
                } else {
                    frames.pop();
                    if let Some((p, _, _)) = frames.last() {
                        let lv = st.low[&v];
                        let lp = st.low[p];
                        st.low.insert(*p, lp.min(lv));
                    }
                    if st.low[&v] == st.index[&v] {
                        let mut scc = Vec::new();
                        while let Some(w) = st.stack.pop() {
                            st.on.remove(&w);
                            scc.push(w);
                            if w == v {
                                break;
                            }
                        }
                        st.out.push(scc);
                    }
                }
            }
        }
        st.out
    }

    /// The size-change principle: every idempotent call graph of a
    /// function to itself in the closure has a strictly decreasing
    /// parameter.
    fn size_change(&self, scc: &[GName]) -> bool {
        type M = Vec<Vec<Option<Rel>>>;
        let members: HashSet<GName> = scc.iter().copied().collect();
        let np = |g: GName| self.facts[&g].nparams;
        let mut set: HashSet<(GName, GName, M)> = HashSet::new();
        let mut any = false;
        for &f in scc {
            for c in &self.facts[&f].calls {
                if !members.contains(&c.callee) {
                    continue;
                }
                any = true;
                let (nf, ng) = (np(f), np(c.callee));
                let mut m: M = vec![vec![None; ng]; nf];
                for (j, rels) in c.rels.iter().enumerate().take(ng) {
                    for (i, r) in rels {
                        if *i < nf {
                            m[*i][j] = m[*i][j].max(Some(*r));
                        }
                    }
                }
                set.insert((f, c.callee, m));
            }
        }
        if !any {
            return true;
        }
        let compose = |a: &M, b: &M| -> M {
            let n = a.len();
            let k = b.first().map(|r| r.len()).unwrap_or(0);
            let mut out: M = vec![vec![None; k]; n];
            for (i, row) in a.iter().enumerate() {
                for (j, x) in row.iter().enumerate() {
                    let Some(x) = x else { continue };
                    for (l, y) in b[j].iter().enumerate() {
                        let Some(y) = y else { continue };
                        let r = if *x == Rel::Lt || *y == Rel::Lt {
                            Rel::Lt
                        } else {
                            Rel::Eq
                        };
                        out[i][l] = out[i][l].max(Some(r));
                    }
                }
            }
            out
        };
        let mut work: Vec<(GName, GName, M)> = set.iter().cloned().collect();
        while let Some((f, g, m)) = work.pop() {
            if set.len() > 4000 {
                return false;
            }
            let next: Vec<(GName, GName, M)> = set
                .iter()
                .filter(|(f2, _, _)| *f2 == g)
                .map(|(_, h, m2)| (f, *h, compose(&m, m2)))
                .chain(
                    set.iter()
                        .filter(|(_, g2, _)| *g2 == f)
                        .map(|(e, _, m0)| (*e, g, compose(m0, &m))),
                )
                .collect();
            for e in next {
                if set.insert(e.clone()) {
                    work.push(e);
                }
            }
        }
        set.iter().all(|(f, g, m)| {
            f != g
                || compose(m, m) != *m
                || (0..m.len()).any(|i| m[i].get(i) == Some(&Some(Rel::Lt)))
        })
    }

    /// Termination of a directly recursive function by its measure, or by
    /// one of its `Int` / `Nat` parameters.
    fn by_measure(&self, f: GName) -> Result<(), Option<String>> {
        let facts = &self.facts[&f];
        let given = self.prog.measures.get(&f);
        let Some(solver) = &self.solver else {
            return Err(given.map(|_| {
                "the measure cannot be checked: Z3 was not found (set `CIRKA_Z3`)".to_string()
            }));
        };
        if let Some(m) = given {
            return measure::check(self.ev, solver, facts, &measure::Measure::Given(m)).map_err(
                |e| {
                    Some(match e {
                        measure::Failure::Counterexample(model) if model.is_empty() => {
                            "the measure does not decrease at a recursive call".to_string()
                        }
                        measure::Failure::Counterexample(model) => {
                            let vs: Vec<String> =
                                model.iter().map(|(k, v)| format!("{k} = {v}")).collect();
                            format!(
                                "the measure does not decrease (or is negative) at a recursive call when {}",
                                vs.join(", ")
                            )
                        }
                        measure::Failure::Unknown(why) => {
                            format!("the measure could not be checked ({why})")
                        }
                        measure::Failure::Partial => {
                            "a recursive call is not applied to all the parameters".to_string()
                        }
                    })
                },
            );
        }
        for i in 0..facts.nparams {
            if matches!(facts.sorts.get(&(i as u32)), Some(Sort::Int | Sort::Nat))
                && measure::check(self.ev, solver, facts, &measure::Measure::Param(i)).is_ok()
            {
                return Ok(());
            }
        }
        Err(None)
    }

    fn is_positive(&mut self, d: GName) -> bool {
        if let Some(p) = self.positive.get(&d) {
            return *p;
        }
        let Global::Data { cons, nparams, .. } = self.prog.get(d) else {
            return true;
        };
        let mut ok = true;
        for c in cons {
            let Global::Con { ty, .. } = self.prog.get(*c) else {
                continue;
            };
            let mut t = ty;
            let mut k = 0;
            while let Term::Pi(_, _, _, a, b) = t {
                if k >= *nparams && negative(a, d, true) {
                    ok = false;
                }
                k += 1;
                t = b;
            }
        }
        self.positive.insert(d, ok);
        ok
    }

    fn not_total(&self, f: GName, why: &Why, span: Span) -> Diagnostic {
        let name = self.prog.short_name(f);
        match why {
            Why::Loops(detail) => {
                let mut d = Diagnostic::error(format!("cannot show that `{name}` terminates"))
                    .with_code("E0502")
                    .with_label(span, "declared `total`");
                d = match detail {
                    Some(m) => d.with_note(m.clone()),
                    None => d.with_note(
                        "no argument becomes structurally smaller along its recursive calls",
                    ),
                };
                d.with_help(
                    "give a measure that decreases: `decreasing e` after a clause \
                     (spec-v0.0.4.md §1.3)",
                )
            }
            _ => Diagnostic::error(format!("`{name}` is declared `total` but is not total"))
                .with_code("E0504")
                .with_label(span, "declared `total`")
                .with_note(self.explain(f, why))
                .with_help("remove what is not total, or declare it `covering`"),
        }
    }

    /// One line on why `f` is not total, following the chain of calls.
    fn explain(&self, f: GName, why: &Why) -> String {
        let mut out = String::new();
        let mut f = f;
        let mut why = why.clone();
        for depth in 0..4 {
            let who = if depth == 0 {
                display(self.prog, f)
            } else {
                "which".to_string()
            };
            let part = match &why {
                Why::Partial => format!("{who} is declared `partial`"),
                Why::Panic => format!("{who} may call `panic`"),
                Why::NoBody => format!("{who} has no definition available"),
                Why::Loops(_) => format!("{who} may not terminate"),
                Why::NonPositive(d) => format!(
                    "{who} uses `{}`, which is not strictly positive",
                    self.prog.short_name(*d)
                ),
                Why::Calls(g) => format!("{who} uses {}", display(self.prog, *g)),
            };
            if depth > 0 {
                out.push_str(", ");
            }
            out.push_str(&part);
            match why {
                Why::Calls(g) => match self.status.get(&g).cloned().flatten() {
                    Some(w) => {
                        f = g;
                        why = w;
                    }
                    None => break,
                },
                _ => break,
            }
        }
        out
    }
}

/// A readable name for messages: generated names of instances and
/// default methods are described.
fn display(prog: &Program, g: GName) -> String {
    let n = prog.short_name(g);
    let parts: Vec<&str> = n.split('$').collect();
    match parts.as_slice() {
        ["impl", tr, _, m] => format!("`{m}` of an `{tr}` instance"),
        ["impl", tr, _] => format!("an `{tr}` instance"),
        [tr, "dm", m] => format!("the default `{m}` of `{tr}`"),
        _ => format!("`{n}`"),
    }
}

/// Whether `d` occurs in a negative position of `t` (to the left of an
/// odd number of arrows).
fn negative(t: &Term, d: GName, pos: bool) -> bool {
    match t {
        Term::Global(g) => *g == d && !pos,
        Term::Pi(_, _, _, a, b) => negative(a, d, !pos) || negative(b, d, pos),
        Term::App(a, b, _) => negative(a, d, pos) || negative(b, d, pos),
        Term::Lam(_, _, _, _, b) => negative(b, d, pos),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
