//! Loops specialized for unique arrays (FEAT-0067).
//!
//! An in-place update of an array parameter checks its uniqueness first
//! (`bytes_unique` / `array_unique`: the array itself when RC == 1,
//! otherwise a copy). A loop written as a self tail call passes the checked
//! array to the next iteration, which checks it again:
//!
//! ```text
//! fn loop(a: ptr @owned, i: i64) -> ptr {
//!   ...
//!   let c = prim bytes_unique(a)
//!   let _ = prim bytes_set_i64(c, i, x)
//!   tailcall loop(c, i + 1)
//! }
//! ```
//!
//! The argument is known to be unique there, but the function cannot tell
//! its callers apart. This pass adds a specialized function `loop$u` that
//! assumes its array parameters are unique and has those checks removed, and
//! makes the tail calls that pass checked arrays go to it. The loop runs the
//! original until an iteration checks the array, and `loop$u` after that. No
//! check is moved, so no array is copied that the original would not copy.
//!
//! The facts are tracked per program point for each owned pointer
//! parameter `p`: the parameter (or a `force` of it, or a check of either)
//! stays *clean* while it is only read and written in place. Copies of a
//! value (`dup`), passing it to anything else or storing it make it
//! *dirty*. In `loop$u` a clean value is `p` itself, unique because every
//! call of `loop$u` passes clean values, so its `force`s and checks are the
//! identity and are removed. The interpreter traps a store into a shared
//! array, so the run tests check the removal.
//!
//! The first iteration need not run in the original either (FEAT-0070):
//! `loop$u` assumes only that its parameters are values with RC == 1, which
//! the entry of `loop` can test without evaluating or writing anything.
//! [`enter_specialized`] adds that test and moves the old body to a cold
//! function `loop$slow`. It runs after the bounds checks are hoisted
//! (`bounds.rs`), so that the test of the loop's precondition stays on the
//! path of the arrays that are not unique, and is made only once, by
//! `loop$u`, on the path of those that are.

use crate::ir::*;
use crate::perceus::{Sigs, instr_uses};
use std::collections::{HashMap, HashSet};

/// Suffix of the specialized function.
pub const SUFFIX: &str = "$u";
/// Suffix of the body of a function entered through its uniqueness test.
pub const SLOW_SUFFIX: &str = "$slow";

/// A function that got a specialized copy, and what the copy assumes.
#[derive(Clone, Debug)]
pub struct Specialized {
    pub func: String,
    pub target: String,
    /// The position of each parameter assumed unique, and whether the
    /// function forces it (the copy has no `force`: it must be a value).
    pub params: Vec<(usize, bool)>,
}

/// Adds the specialized functions to `m` and returns what was specialized.
pub fn specialize_loops(m: &mut Module) -> Vec<Specialized> {
    let sigs = Sigs::new(m);
    let names: HashSet<String> = m.funcs.iter().map(|f| f.name.clone()).collect();
    let mut new = Vec::new();
    let mut done = Vec::new();
    for f in &mut m.funcs {
        if names.contains(&format!("{}{SUFFIX}", f.name)) {
            continue;
        }
        if let Some((w, s)) = specialize(&sigs, f) {
            new.push(w);
            done.push(s);
        }
    }
    m.funcs.extend(new);
    done
}

/// Makes the functions in `done` go to their specialized copies on entry
/// when the parameters are what the copy assumes.
pub fn enter_specialized(m: &mut Module, done: &[Specialized]) {
    let by_name: HashMap<&str, &Specialized> = done.iter().map(|s| (s.func.as_str(), s)).collect();
    let mut slow = Vec::new();
    for f in &mut m.funcs {
        if let Some(s) = by_name.get(f.name.as_str()) {
            slow.push(split_entry(f, s));
        }
    }
    m.funcs.extend(slow);
}

/// Moves the body of `f` to a cold function and returns it. `f` itself is
/// left with the test: it tail calls `s.target` when every assumed
/// parameter is unique (and a value if forced), otherwise the old body,
/// whose self tail calls come back to the test.
///
/// A separate function, not a block of `f`: the copy of a shared array and
/// the evaluation of a thunk are calls, and registers saved for them would
/// be pushed and popped on every entry of the hot path too.
fn split_entry(f: &mut Function, s: &Specialized) -> Function {
    let mut slow = f.clone();
    slow.name = format!("{}{SLOW_SUFFIX}", f.name);
    slow.cold = true;
    let mut instrs = Vec::new();
    let mut all: Option<Var> = None;
    let mut test = |f: &mut Function, prim: Prim, k: usize| {
        let arg = Operand::Var(f.params[k].var);
        let t = f.fresh_var("ent", Ty::I64);
        instrs.push(Instr::Let {
            var: Some(t),
            rhs: Rhs::Prim(prim, vec![arg]),
        });
        all = Some(match all {
            None => t,
            Some(a) => {
                let u = f.fresh_var("ent", Ty::I64);
                instrs.push(Instr::Let {
                    var: Some(u),
                    rhs: Rhs::Prim(Prim::And, vec![Operand::Var(a), Operand::Var(t)]),
                });
                u
            }
        });
    };
    for &(k, forced) in &s.params {
        if forced {
            test(f, Prim::IsValue, k);
        }
        test(f, Prim::IsUnique, k);
    }
    let scrut = all.expect("a specialized function assumes at least one parameter");
    let args: Vec<Operand> = f.params.iter().map(|p| Operand::Var(p.var)).collect();
    let tail = |label: &str, func: String, args: Vec<Operand>| Block {
        label: label.to_string(),
        params: Vec::new(),
        instrs: Vec::new(),
        term: Terminator::TailCall { func, args },
    };
    f.blocks = vec![
        Block {
            label: "entry".to_string(),
            params: Vec::new(),
            instrs,
            term: Terminator::Switch {
                scrut,
                arms: vec![(Case::Int(0), BlockId(1))],
                default: Some(BlockId(2)),
            },
        },
        tail("not_unique", slow.name.clone(), args.clone()),
        tail("unique", s.target.clone(), args),
    ];
    slow
}

/// The state of one parameter at a program point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum St {
    /// `h` is the parameter (or a `force` / check of it), used only in
    /// place so far; `checked` when a check has run on this path.
    Clean {
        h: Var,
        checked: bool,
    },
    Dirty,
}

fn is_check(p: Prim) -> bool {
    matches!(p, Prim::BytesUnique | Prim::ArrayUnique)
}

/// Operations that read or write their first argument (an array) in place
/// without keeping a reference to it (with or without the bounds check).
fn in_place(p: Prim) -> bool {
    use Prim::*;
    matches!(
        p.checked(),
        BytesLen
            | BytesGet
            | BytesSet
            | BytesGetI64
            | BytesSetI64
            | BytesGetF64
            | BytesSetF64
            | ArrayLen
            | ArrayGet
            | ArraySet
    )
}

fn mentions(ops: &[Operand], v: Var) -> usize {
    ops.iter().filter(|o| o.var() == Some(v)).count()
}

/// What the analysis of one parameter found.
#[derive(Default)]
struct Facts {
    /// Removable instructions `(block, index)` in `f$u` (a `force` or a
    /// check of a clean value) and the variable each one defines.
    removable: Vec<(usize, usize, Var)>,
    /// For each block ending in a self tail call: `Some(checked)` when the
    /// parameter's slot gets the clean value.
    tails: HashMap<usize, Option<bool>>,
}

/// The state after instruction `ins` of a parameter in state `st`;
/// `removable` is set when the instruction is a removable `force` / check.
fn step(sigs: &Sigs, st: St, ins: &Instr, removable: &mut bool) -> St {
    let St::Clean { h, checked } = st else {
        return St::Dirty;
    };
    match ins {
        Instr::Let {
            var: Some(r),
            rhs: Rhs::Force(v),
        } if *v == h => {
            *removable = true;
            St::Clean { h: *r, checked }
        }
        Instr::Let {
            var: Some(r),
            rhs: Rhs::Prim(p, args),
        } if is_check(*p) && args.len() == 1 && args[0].var() == Some(h) => {
            *removable = true;
            St::Clean {
                h: *r,
                checked: true,
            }
        }
        Instr::Let {
            rhs: Rhs::Prim(p, args),
            ..
        } if in_place(*p) && args.first().and_then(Operand::var) == Some(h) => {
            if mentions(&args[1..], h) == 0 {
                st
            } else {
                St::Dirty
            }
        }
        _ => {
            if instr_uses(sigs, ins).iter().any(|u| u.0 == h) {
                St::Dirty
            } else {
                st
            }
        }
    }
}

fn join(a: Option<St>, b: St) -> St {
    match a {
        None => b,
        Some(a) if a == b => a,
        Some(_) => St::Dirty,
    }
}

fn analyze(sigs: &Sigs, f: &Function, param: Var) -> Facts {
    let n = f.blocks.len();
    let mut entry: Vec<Option<St>> = vec![None; n];
    if n == 0 {
        return Facts::default();
    }
    entry[0] = Some(St::Clean {
        h: param,
        checked: false,
    });
    // Fixpoint over the block entry states (the lattice has height 2).
    let mut work = vec![0usize];
    while let Some(bi) = work.pop() {
        let b = &f.blocks[bi];
        let mut st = entry[bi].unwrap_or(St::Dirty);
        for ins in &b.instrs {
            st = step(sigs, st, ins, &mut false);
        }
        let out = match (&b.term, st) {
            (Terminator::Jump(_, args), St::Clean { h, .. }) if mentions(args, h) > 0 => St::Dirty,
            (Terminator::Switch { scrut, .. }, St::Clean { h, .. }) if *scrut == h => St::Dirty,
            _ => st,
        };
        for s in b.term.successors() {
            let s = s.0 as usize;
            let j = join(entry[s], out);
            if entry[s] != Some(j) {
                entry[s] = Some(j);
                work.push(s);
            }
        }
    }
    let mut facts = Facts::default();
    for (bi, b) in f.blocks.iter().enumerate() {
        let Some(mut st) = entry[bi] else { continue };
        for (ii, ins) in b.instrs.iter().enumerate() {
            let mut rm = false;
            let next = step(sigs, st, ins, &mut rm);
            if rm && let Some(&r) = ins.defs().first() {
                facts.removable.push((bi, ii, r));
            }
            st = next;
        }
        if let Terminator::TailCall { func, args } = &b.term
            && *func == f.name
        {
            let k = f.params.iter().position(|p| p.var == param);
            let clean = match (st, k) {
                (St::Clean { h, checked }, Some(k))
                    if args.get(k).and_then(Operand::var) == Some(h) && mentions(args, h) == 1 =>
                {
                    Some(checked)
                }
                _ => None,
            };
            facts.tails.insert(bi, clean);
        }
    }
    facts
}

/// The specialized function of `f`, retargeting `f`'s tail calls to it.
fn specialize(sigs: &Sigs, f: &mut Function) -> Option<(Function, Specialized)> {
    let params: Vec<Var> = f
        .params
        .iter()
        .filter(|p| p.own == Own::Owned && f.ty(p.var).is_ptr())
        .map(|p| p.var)
        .collect();
    if params.is_empty() || !has_self_tail_call(f) {
        return None;
    }
    let facts: Vec<(Var, Facts)> = params.iter().map(|&p| (p, analyze(sigs, f, p))).collect();
    // Parameters with a check to remove.
    let cands: Vec<usize> = (0..facts.len())
        .filter(|&i| {
            facts[i].1.removable.iter().any(|&(b, ii, _)| {
                matches!(&f.blocks[b].instrs[ii], Instr::Let { rhs: Rhs::Prim(p, _), .. } if is_check(*p))
            })
        })
        .collect();
    if cands.is_empty() {
        return None;
    }
    // The assumed set: all candidates if some tail call of `f` passes all of
    // them checked, otherwise the first single one that works.
    let enters = |set: &[usize], need_checked: bool, bi: usize| {
        set.iter().all(|&i| match facts[i].1.tails.get(&bi) {
            Some(Some(c)) => *c || !need_checked,
            _ => false,
        })
    };
    let tail_blocks: Vec<usize> = facts[cands[0]].1.tails.keys().copied().collect();
    let works = |set: &[usize]| tail_blocks.iter().any(|&bi| enters(set, true, bi));
    let set: Vec<usize> = if works(&cands) {
        cands
    } else {
        cands.into_iter().find(|&i| works(&[i])).map(|i| vec![i])?
    };
    let wname = format!("{}{SUFFIX}", f.name);
    let assumed = set
        .iter()
        .map(|&i| {
            let (param, fs) = &facts[i];
            let forced = fs.removable.iter().any(|&(b, ii, _)| {
                matches!(
                    &f.blocks[b].instrs[ii],
                    Instr::Let {
                        rhs: Rhs::Force(_),
                        ..
                    }
                )
            });
            let k = f.params.iter().position(|p| p.var == *param);
            (k.expect("a parameter of the function"), forced)
        })
        .collect();
    let mut w = f.clone();
    w.name = wname.clone();
    // Remove the `force`s and checks of clean values in `f$u`.
    let mut subst: HashMap<Var, Var> = HashMap::new();
    let mut gone: HashSet<(usize, usize)> = HashSet::new();
    for &i in &set {
        let (param, fs) = &facts[i];
        for &(b, ii, r) in &fs.removable {
            subst.insert(r, *param);
            gone.insert((b, ii));
        }
    }
    for (bi, b) in w.blocks.iter_mut().enumerate() {
        let instrs = std::mem::take(&mut b.instrs);
        b.instrs = instrs
            .into_iter()
            .enumerate()
            .filter(|(ii, _)| !gone.contains(&(bi, *ii)))
            .map(|(_, ins)| ins)
            .collect();
    }
    rename(&mut w, &subst);
    for bi in tail_blocks {
        if enters(&set, true, bi)
            && let Terminator::TailCall { func, .. } = &mut f.blocks[bi].term
        {
            *func = wname.clone();
        }
        // In `f$u` every clean value is unique, checked or not.
        let to = if enters(&set, false, bi) {
            wname.clone()
        } else {
            f.name.clone()
        };
        if let Terminator::TailCall { func, .. } = &mut w.blocks[bi].term {
            *func = to;
        }
    }
    let info = Specialized {
        func: f.name.clone(),
        target: wname,
        params: assumed,
    };
    Some((w, info))
}

fn has_self_tail_call(f: &Function) -> bool {
    f.blocks
        .iter()
        .any(|b| matches!(&b.term, Terminator::TailCall { func, .. } if *func == f.name))
}

/// Replaces the uses of the variables in `s` (definitions were removed).
fn rename(f: &mut Function, s: &HashMap<Var, Var>) {
    let r = |v: &mut Var| {
        if let Some(t) = s.get(v) {
            *v = *t;
        }
    };
    let ops = |os: &mut Vec<Operand>| {
        for o in os {
            if let Operand::Var(v) = o
                && let Some(t) = s.get(v)
            {
                *v = *t;
            }
        }
    };
    for b in &mut f.blocks {
        for ins in &mut b.instrs {
            match ins {
                Instr::Let { rhs, .. } => match rhs {
                    Rhs::Lit(_) | Rhs::Global(_) => {}
                    Rhs::Prim(_, a)
                    | Rhs::Call { args: a, .. }
                    | Rhs::MkClosure { args: a, .. }
                    | Rhs::MkThunk { args: a, .. }
                    | Rhs::Extern { args: a, .. } => ops(a),
                    Rhs::Alloc { args, reuse, .. } => {
                        ops(args);
                        if let Some(t) = reuse {
                            r(t);
                        }
                    }
                    Rhs::Proj { obj, .. } => r(obj),
                    Rhs::Apply { clo, args } => {
                        r(clo);
                        ops(args);
                    }
                    Rhs::Force(v) | Rhs::DropReuse(v) => r(v),
                },
                Instr::LetMulti { args, .. } => ops(args),
                Instr::Dup(v) | Instr::Drop(v) => r(v),
                Instr::Set { obj, val, .. } => {
                    r(obj);
                    if let Operand::Var(v) = val
                        && let Some(t) = s.get(v)
                    {
                        *v = *t;
                    }
                }
            }
        }
        match &mut b.term {
            Terminator::Ret(Operand::Var(v)) => r(v),
            Terminator::Ret(_) | Terminator::Panic(_) | Terminator::Unreachable => {}
            Terminator::RetMulti(a)
            | Terminator::Jump(_, a)
            | Terminator::TailCall { args: a, .. } => ops(a),
            Terminator::Switch { scrut, .. } => r(scrut),
        }
    }
}
