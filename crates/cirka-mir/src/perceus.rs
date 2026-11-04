//! Perceus reference counting (04-memory-model.md §2.1): inserts precise
//! `dup` / `drop` instructions into MIR that does not contain any.
//!
//! Input semantics: every pointer variable denotes a reference; the pass
//! decides where references are copied and released.
//!
//! - Owned variables (owned parameters, block parameters, results of
//!   allocating instructions) hold exactly one reference while they are live.
//!   A consuming use that is not the last use is preceded by `dup`; a
//!   borrowing last use is followed by `drop`; dead definitions are dropped
//!   immediately.
//! - Borrowed variables (borrowed parameters, `proj` / `array_get` results)
//!   are never dropped; each consuming use is preceded by `dup`. A use of a
//!   projected variable counts as a (borrowing) use of its source object, so
//!   the source stays alive as long as the projection is used (Lean 4's
//!   borrowed projections; drop specialization later fuses the pairs).
//! - Variables that die on a `switch` edge are dropped at the start of the
//!   target block (the edge is split when the target has other predecessors).
//!
//! Functions that already contain `dup`, `drop`, `drop_reuse` or `reuse` are
//! considered hand-annotated and left unchanged.

use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// How an instruction uses a variable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Consume,
    Borrow,
}

/// Ownership signatures of functions and externs (`None` for unknown callees).
pub struct Sigs {
    pub funcs: HashMap<String, Vec<Own>>,
    pub externs: HashMap<String, Vec<Own>>,
}

impl Sigs {
    pub fn new(m: &Module) -> Sigs {
        Sigs {
            funcs: m
                .funcs
                .iter()
                .map(|f| (f.name.clone(), f.params.iter().map(|p| p.own).collect()))
                .collect(),
            externs: m
                .externs
                .iter()
                .map(|e| (e.name.clone(), e.params.iter().map(|p| p.1).collect()))
                .collect(),
        }
    }

    fn func_own(&self, f: &str, i: usize) -> Mode {
        own_mode(self.funcs.get(f).and_then(|o| o.get(i)))
    }

    fn extern_own(&self, f: &str, i: usize) -> Mode {
        own_mode(self.externs.get(f).and_then(|o| o.get(i)))
    }
}

fn own_mode(o: Option<&Own>) -> Mode {
    match o {
        Some(Own::Borrowed) => Mode::Borrow,
        _ => Mode::Consume,
    }
}

fn push_ops(out: &mut Vec<(Var, Mode)>, ops: &[Operand], mode: impl Fn(usize) -> Mode) {
    for (i, o) in ops.iter().enumerate() {
        if let Operand::Var(v) = o {
            out.push((*v, mode(i)));
        }
    }
}

/// Variables used by a right-hand side, with their use mode.
pub fn rhs_uses(sigs: &Sigs, rhs: &Rhs) -> Vec<(Var, Mode)> {
    let mut u = Vec::new();
    match rhs {
        Rhs::Lit(_) | Rhs::Global(_) => {}
        Rhs::Prim(p, args) => push_ops(&mut u, args, |i| {
            if p.consumes().contains(&i) {
                Mode::Consume
            } else {
                Mode::Borrow
            }
        }),
        Rhs::Alloc { args, reuse, .. } => {
            push_ops(&mut u, args, |_| Mode::Consume);
            if let Some(t) = reuse {
                u.push((*t, Mode::Consume));
            }
        }
        Rhs::Proj { obj, .. } => u.push((*obj, Mode::Borrow)),
        Rhs::Call { func, args } => push_ops(&mut u, args, |i| sigs.func_own(func, i)),
        Rhs::Apply { clo, args } => {
            u.push((*clo, Mode::Consume));
            push_ops(&mut u, args, |_| Mode::Consume);
        }
        Rhs::MkClosure { args, .. } | Rhs::MkThunk { args, .. } => {
            push_ops(&mut u, args, |_| Mode::Consume)
        }
        Rhs::Force(v) | Rhs::DropReuse(v) => u.push((*v, Mode::Consume)),
        Rhs::Extern { func, args } => push_ops(&mut u, args, |i| sigs.extern_own(func, i)),
    }
    u
}

/// Variables used by an instruction.
pub fn instr_uses(sigs: &Sigs, ins: &Instr) -> Vec<(Var, Mode)> {
    match ins {
        Instr::Let { rhs, .. } => rhs_uses(sigs, rhs),
        Instr::Dup(v) => vec![(*v, Mode::Borrow)],
        Instr::Drop(v) => vec![(*v, Mode::Consume)],
        Instr::Set { obj, val, .. } => {
            let mut u = vec![(*obj, Mode::Borrow)];
            if let Operand::Var(v) = val {
                u.push((*v, Mode::Consume));
            }
            u
        }
    }
}

/// Variables used by a terminator.
pub fn term_uses(sigs: &Sigs, t: &Terminator) -> Vec<(Var, Mode)> {
    let mut u = Vec::new();
    match t {
        Terminator::Ret(Operand::Var(v)) => u.push((*v, Mode::Consume)),
        Terminator::Ret(_) | Terminator::Panic(_) | Terminator::Unreachable => {}
        Terminator::Jump(_, args) => push_ops(&mut u, args, |_| Mode::Consume),
        Terminator::Switch { scrut, .. } => u.push((*scrut, Mode::Borrow)),
        Terminator::TailCall { func, args } => push_ops(&mut u, args, |i| sigs.func_own(func, i)),
    }
    u
}

/// True if the function already contains reference-counting instructions.
pub fn has_rc(f: &Function) -> bool {
    f.blocks.iter().any(|b| {
        b.instrs.iter().any(|i| {
            matches!(
                i,
                Instr::Dup(_)
                    | Instr::Drop(_)
                    | Instr::Let {
                        rhs: Rhs::DropReuse(_) | Rhs::Alloc { reuse: Some(_), .. },
                        ..
                    }
            )
        })
    })
}

/// The whole Perceus pipeline on MIR without reference counting:
/// borrow inference, then dup/drop insertion.
pub fn run(m: &mut Module) {
    crate::borrow::infer_borrow(m);
    insert_rc(m);
}

/// Inserts dup/drop into every function of `m` that has none.
pub fn insert_rc(m: &mut Module) {
    let sigs = Sigs::new(m);
    for f in &mut m.funcs {
        if !has_rc(f) {
            insert_fn(&sigs, f);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Scalar,
    Owned,
    Borrowed,
}

/// Per-variable ownership kind and projection parent.
pub struct VarInfo {
    pub kind: Vec<Kind>,
    pub parent: Vec<Option<Var>>,
}

impl VarInfo {
    pub fn new(f: &Function) -> VarInfo {
        let n = f.var_names.len();
        let mut kind = vec![Kind::Scalar; n];
        let mut parent = vec![None; n];
        for p in &f.params {
            if f.ty(p.var).is_ptr() {
                kind[p.var.0 as usize] = match p.own {
                    Own::Owned => Kind::Owned,
                    Own::Borrowed => Kind::Borrowed,
                };
            }
        }
        for b in &f.blocks {
            for p in &b.params {
                if f.ty(*p).is_ptr() {
                    kind[p.0 as usize] = Kind::Owned;
                }
            }
            for ins in &b.instrs {
                if let Instr::Let { var: Some(v), rhs } = ins
                    && f.ty(*v).is_ptr()
                {
                    let src = match rhs {
                        Rhs::Proj { obj, .. } => Some(*obj),
                        Rhs::Prim(p, args) if p.result_borrowed() => {
                            args.first().and_then(|a| a.var())
                        }
                        _ => None,
                    };
                    kind[v.0 as usize] = if src.is_some() {
                        Kind::Borrowed
                    } else {
                        Kind::Owned
                    };
                    parent[v.0 as usize] = src;
                }
            }
        }
        VarInfo { kind, parent }
    }

    fn kind(&self, v: Var) -> Kind {
        self.kind[v.0 as usize]
    }

    /// Adds borrowing uses of the projection sources of every used variable.
    pub fn expand(&self, uses: &mut Vec<(Var, Mode)>) {
        let n = uses.len();
        for i in 0..n {
            let mut v = uses[i].0;
            let mut depth = 0;
            while let Some(p) = self.parent[v.0 as usize] {
                uses.push((p, Mode::Borrow));
                v = p;
                depth += 1;
                if depth > 1000 {
                    break;
                }
            }
        }
    }
}

type Set = BTreeSet<Var>;

/// Liveness of pointer variables (with projection-expanded uses).
pub struct Liveness {
    /// Live variables at block entry (excluding the block's parameters).
    pub live_in: Vec<Set>,
    /// Live variables at the end of each block (after the terminator's uses).
    pub live_out: Vec<Set>,
}

fn ptr_uses(f: &Function, info: &VarInfo, mut uses: Vec<(Var, Mode)>) -> Vec<(Var, Mode)> {
    info.expand(&mut uses);
    uses.retain(|(v, _)| f.ty(*v).is_ptr());
    uses
}

fn instr_def(ins: &Instr) -> Option<Var> {
    match ins {
        Instr::Let { var, .. } => *var,
        _ => None,
    }
}

pub fn liveness(sigs: &Sigs, f: &Function, info: &VarInfo) -> Liveness {
    let n = f.blocks.len();
    let mut live_in = vec![Set::new(); n];
    let mut live_out = vec![Set::new(); n];
    let mut changed = true;
    while changed {
        changed = false;
        for bi in (0..n).rev() {
            let b = &f.blocks[bi];
            let mut out = Set::new();
            for s in b.term.successors() {
                if let Some(li) = live_in.get(s.0 as usize) {
                    out.extend(li.iter().copied());
                }
            }
            let mut live = out.clone();
            for (v, _) in ptr_uses(f, info, term_uses(sigs, &b.term)) {
                live.insert(v);
            }
            for ins in b.instrs.iter().rev() {
                if let Some(d) = instr_def(ins) {
                    live.remove(&d);
                }
                for (v, _) in ptr_uses(f, info, instr_uses(sigs, ins)) {
                    live.insert(v);
                }
            }
            for p in &b.params {
                live.remove(p);
            }
            if live != live_in[bi] || out != live_out[bi] {
                live_in[bi] = live;
                live_out[bi] = out;
                changed = true;
            }
        }
    }
    Liveness { live_in, live_out }
}

/// Dups needed before and drops needed after one instruction / terminator.
fn plan(info: &VarInfo, uses: &[(Var, Mode)], live_after: &Set) -> (Vec<Var>, Vec<Var>) {
    let mut counts: BTreeMap<Var, (u32, u32)> = BTreeMap::new();
    let mut order = Vec::new();
    for (v, m) in uses {
        let e = counts.entry(*v).or_insert_with(|| {
            order.push(*v);
            (0, 0)
        });
        match m {
            Mode::Consume => e.0 += 1,
            Mode::Borrow => e.1 += 1,
        }
    }
    let mut dups = Vec::new();
    let mut drops = Vec::new();
    for v in order {
        let (cc, bc) = counts[&v];
        match info.kind(v) {
            Kind::Scalar => {}
            Kind::Borrowed => dups.extend(std::iter::repeat_n(v, cc as usize)),
            Kind::Owned => {
                let keep = live_after.contains(&v);
                if cc == 0 {
                    if !keep {
                        drops.push(v);
                    }
                } else if keep || bc > 0 {
                    dups.extend(std::iter::repeat_n(v, cc as usize));
                    if !keep {
                        drops.push(v);
                    }
                } else {
                    dups.extend(std::iter::repeat_n(v, cc as usize - 1));
                }
            }
        }
    }
    (dups, drops)
}

fn owned_dead(info: &VarInfo, vars: &[Var], live: &Set) -> Vec<Var> {
    vars.iter()
        .copied()
        .filter(|v| info.kind(*v) == Kind::Owned && !live.contains(v))
        .collect()
}

/// Inserts dup/drop into one function.
pub fn insert_fn(sigs: &Sigs, f: &mut Function) {
    let info = VarInfo::new(f);
    let lv = liveness(sigs, f, &info);
    let n = f.blocks.len();
    let preds = f.predecessors();
    // Drops to prepend to blocks (from switch edges).
    let mut entry_drops: Vec<Vec<Var>> = vec![Vec::new(); n];
    let mut new_blocks: Vec<Block> = Vec::new();
    let mut blocks = std::mem::take(&mut f.blocks);
    for (bi, b) in blocks.iter_mut().enumerate() {
        let instrs = std::mem::take(&mut b.instrs);
        // Live sets after each instruction, computed backwards.
        let mut live = lv.live_out[bi].clone();
        for (v, _) in ptr_uses(f, &info, term_uses(sigs, &b.term)) {
            live.insert(v);
        }
        let before_term = live.clone();
        let mut after: Vec<Set> = vec![Set::new(); instrs.len()];
        for (i, ins) in instrs.iter().enumerate().rev() {
            after[i] = live.clone();
            if let Some(d) = instr_def(ins) {
                live.remove(&d);
            }
            for (v, _) in ptr_uses(f, &info, instr_uses(sigs, ins)) {
                live.insert(v);
            }
        }
        let mut out = Vec::new();
        // Dead parameters.
        let params: Vec<Var> = if bi == 0 {
            f.params.iter().map(|p| p.var).collect()
        } else {
            b.params.clone()
        };
        for v in owned_dead(&info, &params, &live) {
            out.push(Instr::Drop(v));
        }
        for (i, ins) in instrs.into_iter().enumerate() {
            let uses = ptr_uses(f, &info, instr_uses(sigs, &ins));
            let (dups, drops) = plan(&info, &uses, &after[i]);
            let def = instr_def(&ins);
            out.extend(dups.into_iter().map(Instr::Dup));
            out.push(ins);
            if let Some(d) = def
                && f.ty(d).is_ptr()
            {
                out.extend(
                    owned_dead(&info, &[d], &after[i])
                        .into_iter()
                        .map(Instr::Drop),
                );
            }
            out.extend(drops.into_iter().map(Instr::Drop));
        }
        // Terminator.
        let term = std::mem::replace(&mut b.term, Terminator::Unreachable);
        let tuses = ptr_uses(f, &info, term_uses(sigs, &term));
        match term {
            Terminator::Switch {
                scrut,
                arms,
                default,
            } => {
                let held: Vec<Var> = before_term
                    .iter()
                    .copied()
                    .filter(|v| info.kind(*v) == Kind::Owned)
                    .collect();
                let mut targets: Vec<BlockId> = arms.iter().map(|a| a.1).collect();
                targets.extend(default.iter().copied());
                let mut fix = |t: BlockId, new_blocks: &mut Vec<Block>| -> BlockId {
                    let ti = t.0 as usize;
                    let drops = owned_dead(&info, &held, &lv.live_in[ti]);
                    if drops.is_empty() {
                        return t;
                    }
                    let single =
                        targets.iter().filter(|x| **x == t).count() == 1 && preds[ti].len() == 1;
                    if single {
                        entry_drops[ti] = drops;
                        t
                    } else {
                        let id = BlockId((n + new_blocks.len()) as u32);
                        new_blocks.push(Block {
                            label: format!("{}.e{}", b.label, new_blocks.len()),
                            params: vec![],
                            instrs: drops.into_iter().map(Instr::Drop).collect(),
                            term: Terminator::Jump(t, vec![]),
                        });
                        id
                    }
                };
                let arms = arms
                    .iter()
                    .map(|(c, t)| (c.clone(), fix(*t, &mut new_blocks)))
                    .collect();
                let default = default.map(|d| fix(d, &mut new_blocks));
                b.term = Terminator::Switch {
                    scrut,
                    arms,
                    default,
                };
            }
            Terminator::Jump(t, args) => {
                let (dups, drops) = plan(&info, &tuses, &lv.live_out[bi]);
                out.extend(dups.into_iter().map(Instr::Dup));
                out.extend(drops.into_iter().map(Instr::Drop));
                b.term = Terminator::Jump(t, args);
            }
            Terminator::TailCall { func, args } => {
                let (dups, drops) = plan(&info, &tuses, &Set::new());
                out.extend(dups.into_iter().map(Instr::Dup));
                if drops.is_empty() {
                    b.term = Terminator::TailCall { func, args };
                } else {
                    // Borrowed arguments must outlive the call: turn the
                    // tail call into call + drops + ret.
                    let r = f.fresh_var("r", f.ret);
                    out.push(Instr::Let {
                        var: Some(r),
                        rhs: Rhs::Call { func, args },
                    });
                    out.extend(drops.into_iter().map(Instr::Drop));
                    b.term = Terminator::Ret(Operand::Var(r));
                }
            }
            t => {
                let (dups, drops) = plan(&info, &tuses, &Set::new());
                out.extend(dups.into_iter().map(Instr::Dup));
                out.extend(drops.into_iter().map(Instr::Drop));
                b.term = t;
            }
        }
        b.instrs = out;
    }
    for (bi, d) in entry_drops.into_iter().enumerate() {
        if !d.is_empty() {
            let b = &mut blocks[bi];
            let mut instrs: Vec<Instr> = d.into_iter().map(Instr::Drop).collect();
            instrs.append(&mut b.instrs);
            b.instrs = instrs;
        }
    }
    blocks.extend(new_blocks);
    f.blocks = blocks;
}

/// Removes all reference-counting instructions (used to test the pass on
/// hand-annotated programs).
pub fn strip_rc(m: &mut Module) {
    for f in &mut m.funcs {
        for b in &mut f.blocks {
            b.instrs.retain(|i| {
                !matches!(
                    i,
                    Instr::Dup(_)
                        | Instr::Drop(_)
                        | Instr::Let {
                            rhs: Rhs::DropReuse(_),
                            ..
                        }
                )
            });
            for i in &mut b.instrs {
                if let Instr::Let {
                    rhs: Rhs::Alloc { reuse, .. },
                    ..
                } = i
                {
                    *reuse = None;
                }
            }
        }
    }
}
