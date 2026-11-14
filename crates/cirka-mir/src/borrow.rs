//! Borrow inference (04-memory-model.md §2.2, Lean 4's `inferBorrow`).
//!
//! Runs on MIR without dup/drop, before [`crate::perceus::insert_rc`].
//! Pointer parameters of internal functions start out borrowed and become
//! owned when the function body
//!
//! - consumes them (returns them, stores them in a constructor, passes them
//!   to an owned parameter, captures them, forces them, ...),
//! - consumes a projection of them (so the cell can be reused, FEAT-0022),
//! - passes them to an owned parameter of a tail call (keeps the tail call),
//!
//! and owned arguments of tail calls make the callee's parameter owned too
//! (otherwise the tail call would have to become a normal call followed by a
//! `drop`). The analysis is a monotone fixed point over all functions.
//!
//! A parameter that stays borrowed then becomes owned when no call gains
//! from borrowing it ([`owned_by_callers`], FEAT-0083): every call passes
//! the last use of an owned value, which the caller would only drop after
//! the call. Owning it, the callee frees the value as it reads it (Koka's
//! default) instead of the caller freeing it in a second traversal.
//!
//! Fixed signatures (never changed): `main`, global initializers, the
//! non-captured parameters of closure functions, functions with hand-written
//! reference counting and every function they call.

use crate::ir::*;
use crate::perceus::{
    Kind, Liveness, Mode, Sigs, VarInfo, has_rc, instr_uses, liveness, term_uses,
};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Infers borrowed parameters for all non-fixed functions of `m`.
pub fn infer_borrow(m: &mut Module) {
    let fixed = fixed_functions(m);
    // Start: every pointer parameter of an inferred function is borrowed.
    for f in &mut m.funcs {
        if fixed.contains(&f.name) {
            continue;
        }
        let tys = f.var_tys.clone();
        for p in &mut f.params {
            if tys[p.var.0 as usize].is_ptr() {
                p.own = Own::Borrowed;
            }
        }
    }
    // Closure functions: parameters after the captured ones must be owned.
    for (func, ncaptured) in closure_targets(m) {
        if let Some(f) = m.funcs.iter_mut().find(|f| f.name == func) {
            for p in f.params.iter_mut().skip(ncaptured) {
                p.own = Own::Owned;
            }
        }
    }
    loop {
        let sigs = Sigs::new(m);
        let mut changes: Vec<(String, usize)> = Vec::new();
        for f in &m.funcs {
            if has_rc(f) {
                continue;
            }
            changes.extend(owned_params(&sigs, f, &fixed));
        }
        let mut changed = false;
        for (name, i) in changes {
            if let Some(f) = m.funcs.iter_mut().find(|f| f.name == name)
                && f.params[i].own == Own::Borrowed
            {
                f.params[i].own = Own::Owned;
                changed = true;
            }
        }
        if !changed {
            if owned_by_callers(m, &fixed) {
                continue;
            }
            break;
        }
    }
}

/// A call of a function: the caller, its block and the instruction (`None`
/// for the terminator).
type Site = (usize, usize, Option<usize>);

/// Makes owned every borrowed parameter that no call gains from borrowing:
/// each call passes either the last use of an owned value, or a field of
/// an owned value that the caller only takes apart (Perceus dups the field
/// right after the match, and dropping the unique parent then frees just
/// its cell), where a parameter of the caller that becomes owned too counts
/// as owned (the recursive calls of a traversal pass the fields of its own
/// parameter: a greatest fixed point). Borrowing would save no `dup` at
/// such calls, and owning lets the callee free the value while reading it
/// instead of the caller freeing it afterwards in a second traversal
/// (binary-trees' `check`). A static object (a string literal) is not
/// given up: borrowed, it costs nothing. A thunk is evaluated once: the
/// parameters of its function (the captured values) are given up by the
/// evaluation, which moves owned ones out of the thunk. Returns whether a
/// parameter changed.
fn owned_by_callers(m: &mut Module, fixed: &HashSet<String>) -> bool {
    let sigs = Sigs::new(m);
    let index: HashMap<&str, usize> = m
        .funcs
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    // Closure functions keep their signatures (a closure is applied any
    // number of times); thunk functions are entered once.
    let mut closures: HashSet<&str> = HashSet::new();
    let mut thunks: HashSet<&str> = HashSet::new();
    let mut sites: HashMap<usize, Vec<Site>> = HashMap::new();
    for (fi, f) in m.funcs.iter().enumerate() {
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, ins) in b.instrs.iter().enumerate() {
                match ins {
                    Instr::Let {
                        rhs: Rhs::Call { func, .. },
                        ..
                    }
                    | Instr::LetMulti { func, .. } => {
                        if let Some(&g) = index.get(func.as_str()) {
                            sites.entry(g).or_default().push((fi, bi, Some(ii)));
                        }
                    }
                    Instr::Let {
                        rhs: Rhs::MkClosure { func, .. },
                        ..
                    } => {
                        closures.insert(func);
                    }
                    Instr::Let {
                        rhs: Rhs::MkThunk { func, .. },
                        ..
                    } => {
                        thunks.insert(func);
                    }
                    _ => {}
                }
            }
            if let Terminator::TailCall { func, .. } = &b.term
                && let Some(&g) = index.get(func.as_str())
            {
                sites.entry(g).or_default().push((fi, bi, None));
            }
        }
    }
    let mut cand: HashSet<(usize, usize)> = HashSet::new();
    for (fi, f) in m.funcs.iter().enumerate() {
        let thunk = thunks.contains(f.name.as_str());
        if fixed.contains(&f.name)
            || closures.contains(f.name.as_str())
            || !(thunk || sites.contains_key(&fi))
        {
            continue;
        }
        for (pi, p) in f.params.iter().enumerate() {
            if p.own == Own::Borrowed {
                cand.insert((fi, pi));
            }
        }
    }
    if cand.is_empty() {
        return false;
    }
    let mut callers: HashMap<usize, Caller> = HashMap::new();
    loop {
        let mut out = Vec::new();
        for &(fi, pi) in &cand {
            let none = Vec::new();
            let ok = sites.get(&fi).unwrap_or(&none).iter().all(|&(gi, bi, at)| {
                let c = callers
                    .entry(gi)
                    .or_insert_with(|| Caller::new(&sigs, &m.funcs[gi]));
                c.gives_up(&m.funcs[gi], gi, bi, at, pi, &cand)
            });
            if !ok {
                out.push((fi, pi));
            }
        }
        if out.is_empty() {
            break;
        }
        for x in out {
            cand.remove(&x);
        }
    }
    for &(fi, pi) in &cand {
        m.funcs[fi].params[pi].own = Own::Owned;
    }
    !cand.is_empty()
}

/// What deciding [`owned_by_callers`] needs to know of a calling function.
struct Caller {
    info: VarInfo,
    live: Liveness,
    /// The object each projection is taken from.
    proj_of: HashMap<Var, Var>,
    /// Variables used other than by projecting from or switching on them.
    used_whole: HashSet<Var>,
    /// Live variables after each instruction of a block (computed on
    /// demand).
    after: HashMap<usize, Vec<BTreeSet<Var>>>,
    sigs_uses: Vec<Vec<Vec<(Var, Mode)>>>,
    term_uses: Vec<Vec<(Var, Mode)>>,
}

impl Caller {
    fn new(sigs: &Sigs, f: &Function) -> Caller {
        let info = VarInfo::new(f);
        let live = liveness(sigs, f, &info);
        let mut proj_of = HashMap::new();
        let mut used_whole = HashSet::new();
        let mut sigs_uses = Vec::new();
        let mut term_uses_v = Vec::new();
        for b in &f.blocks {
            let mut per = Vec::new();
            for ins in &b.instrs {
                let mut uses = instr_uses(sigs, ins);
                if let Instr::Let {
                    var: Some(v),
                    rhs: Rhs::Proj { obj, .. },
                } = ins
                {
                    proj_of.insert(*v, *obj);
                } else {
                    used_whole.extend(uses.iter().map(|(v, _)| *v));
                }
                info.expand(&mut uses);
                uses.retain(|(v, _)| f.ty(*v).is_ptr());
                per.push(uses);
            }
            sigs_uses.push(per);
            let tu = term_uses(sigs, &b.term);
            if !matches!(b.term, Terminator::Switch { .. }) {
                used_whole.extend(tu.iter().map(|(v, _)| *v));
            }
            let mut tu = tu;
            info.expand(&mut tu);
            tu.retain(|(v, _)| f.ty(*v).is_ptr());
            term_uses_v.push(tu);
        }
        Caller {
            info,
            live,
            proj_of,
            used_whole,
            after: HashMap::new(),
            sigs_uses,
            term_uses: term_uses_v,
        }
    }

    /// Live pointer variables after instruction `at` of block `bi` (after
    /// the terminator for `None`).
    fn live_after(&mut self, f: &Function, bi: usize, at: Option<usize>) -> BTreeSet<Var> {
        let Some(at) = at else {
            return self.live.live_out[bi].clone();
        };
        let n = f.blocks[bi].instrs.len();
        let sets = self.after.entry(bi).or_insert_with(|| {
            let mut live: BTreeSet<Var> = self.live.live_out[bi].clone();
            live.extend(self.term_uses[bi].iter().map(|(v, _)| *v));
            let mut sets = vec![BTreeSet::new(); n];
            for i in (0..n).rev() {
                sets[i] = live.clone();
                for d in f.blocks[bi].instrs[i].defs() {
                    live.remove(d);
                }
                live.extend(self.sigs_uses[bi][i].iter().map(|(v, _)| *v));
            }
            sets
        });
        sets[at].clone()
    }

    /// Whether the call at (`bi`, `at`) of `f` (function `gi`) gives up its
    /// argument `pi`: see [`owned_by_callers`].
    fn gives_up(
        &mut self,
        f: &Function,
        gi: usize,
        bi: usize,
        at: Option<usize>,
        pi: usize,
        cand: &HashSet<(usize, usize)>,
    ) -> bool {
        let b = &f.blocks[bi];
        let args = match at {
            Some(i) => match &b.instrs[i] {
                Instr::Let {
                    rhs: Rhs::Call { args, .. },
                    ..
                }
                | Instr::LetMulti { args, .. } => args,
                _ => return false,
            },
            None => match &b.term {
                Terminator::TailCall { args, .. } => args,
                _ => return false,
            },
        };
        let v = match args.get(pi) {
            Some(Operand::Var(v)) => *v,
            // A static object (a string literal): borrowed, the call costs
            // nothing; owned, the callee drops it (a check that it is
            // static on every call).
            Some(Operand::Const(_)) => return false,
            None => return false,
        };
        if args.iter().filter(|a| a.var() == Some(v)).count() != 1 {
            return false;
        }
        // Owned, or a parameter of the caller that becomes owned too.
        let owned = |x: Var, info: &VarInfo| match info.kind[x.0 as usize] {
            Kind::Owned => true,
            Kind::Borrowed => f
                .params
                .iter()
                .position(|p| p.var == x)
                .is_some_and(|pj| cand.contains(&(gi, pj))),
            Kind::Scalar => false,
        };
        if self.proj_of.contains_key(&v) {
            // A field: its object (through projections) is owned and only
            // taken apart.
            let mut o = v;
            let mut depth = 0;
            while let Some(&p) = self.proj_of.get(&o) {
                o = p;
                depth += 1;
                if depth > 1000 {
                    return false;
                }
            }
            return owned(o, &self.info)
                && !self.used_whole.contains(&o)
                && !self.live_after(f, bi, at).contains(&v);
        }
        owned(v, &self.info) && !self.live_after(f, bi, at).contains(&v)
    }
}

fn closure_targets(m: &Module) -> Vec<(String, usize)> {
    let mut v = Vec::new();
    for f in &m.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::Let {
                    rhs: Rhs::MkClosure { func, args },
                    ..
                } = i
                {
                    v.push((func.clone(), args.len()));
                }
            }
        }
    }
    v
}

fn fixed_functions(m: &Module) -> HashSet<String> {
    let mut fixed: HashSet<String> = HashSet::new();
    fixed.insert("main".into());
    for g in &m.globals {
        fixed.insert(g.init.clone());
    }
    for f in &m.funcs {
        if !has_rc(f) {
            continue;
        }
        fixed.insert(f.name.clone());
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::Let {
                    rhs:
                        Rhs::Call { func, .. } | Rhs::MkClosure { func, .. } | Rhs::MkThunk { func, .. },
                    ..
                } = i
                {
                    fixed.insert(func.clone());
                }
                if let Instr::LetMulti { func, .. } = i {
                    fixed.insert(func.clone());
                }
            }
            if let Terminator::TailCall { func, .. } = &b.term {
                fixed.insert(func.clone());
            }
        }
    }
    fixed
}

/// Parameters (of `f` and of tail-called functions) that must become owned.
fn owned_params(sigs: &Sigs, f: &Function, fixed: &HashSet<String>) -> Vec<(String, usize)> {
    let info = VarInfo::new(f);
    let mut owned = vec![false; f.var_names.len()];
    let mark = |v: Var, owned: &mut Vec<bool>| {
        // The variable and the sources of its projections.
        let mut v = v;
        let mut depth = 0;
        loop {
            owned[v.0 as usize] = true;
            match info.parent[v.0 as usize] {
                Some(p) if depth < 1000 => {
                    v = p;
                    depth += 1;
                }
                _ => break,
            }
        }
    };
    for b in &f.blocks {
        for i in &b.instrs {
            for (v, m) in instr_uses(sigs, i) {
                if m == Mode::Consume && f.ty(v).is_ptr() {
                    mark(v, &mut owned);
                }
            }
        }
        for (v, m) in term_uses(sigs, &b.term) {
            if m == Mode::Consume && f.ty(v).is_ptr() {
                mark(v, &mut owned);
            }
        }
    }
    let mut out = Vec::new();
    for (i, p) in f.params.iter().enumerate() {
        if p.own == Own::Borrowed && owned[p.var.0 as usize] {
            out.push((f.name.clone(), i));
        }
    }
    // Preserve tail calls: owned arguments need owned callee parameters.
    for b in &f.blocks {
        if let Terminator::TailCall { func, args } = &b.term
            && !fixed.contains(func)
        {
            for (i, a) in args.iter().enumerate() {
                if let Operand::Var(v) = a
                    && info.kind[v.0 as usize] == Kind::Owned
                {
                    out.push((func.clone(), i));
                }
            }
        }
    }
    out
}
