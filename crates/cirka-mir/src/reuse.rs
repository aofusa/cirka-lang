//! Reuse analysis and dup/drop fusion (04-memory-model.md §2.3), run after
//! [`crate::perceus::insert_rc`].
//!
//! - **Reuse (FBIP)**: `drop x`, where `x` is known to be a constructor of
//!   size S, followed in the same block by an `alloc` of a (non-nullary)
//!   constructor of size S becomes `let t = drop_reuse x` and the allocation
//!   `reuse t`. When `x` is unique at run time its memory is recycled.
//! - **Fusion**: `dup x` followed by `drop x` in the same block cancel out
//!   when nothing in between consumes `x` (for projections only when they
//!   are adjacent, since a projection relies on its source while borrowed).
//!
//! Drop specialization (`dup` of the fields followed by `drop` /
//! `drop_reuse` of the object) is done by the LLVM backend, which turns the
//! sequence into a uniqueness test (07-backend.md).

use crate::ir::*;
use crate::perceus::{Mode, Sigs, instr_uses};
use std::collections::HashMap;

/// Runs reuse analysis and fusion on every function.
pub fn optimize(m: &mut Module) {
    let sigs = Sigs::new(m);
    let types = m.types.clone();
    let tm = Module {
        types,
        externs: Vec::new(),
        globals: Vec::new(),
        funcs: Vec::new(),
        collect_cycles: false,
    };
    for f in &mut m.funcs {
        fuse(&sigs, f);
        reuse(&tm, f);
        sink_dups(&sigs, f);
    }
}

/// Constructor known for variables at the start of each block (from a
/// `switch` arm that is the block's only predecessor edge), mirroring the
/// verifier's rules.
fn entry_known(f: &Function) -> Vec<Option<(Var, CtorRef)>> {
    let preds = f.predecessors();
    let mut known = vec![None; f.blocks.len()];
    for b in &f.blocks {
        if let Terminator::Switch {
            scrut,
            arms,
            default,
        } = &b.term
        {
            let mut targets: Vec<BlockId> = arms.iter().map(|a| a.1).collect();
            targets.extend(default.iter().copied());
            for (c, t) in arms {
                let ti = t.0 as usize;
                if let Case::Ctor(c) = c
                    && targets.iter().filter(|x| *x == t).count() == 1
                    && preds[ti].len() == 1
                {
                    known[ti] = Some((*scrut, *c));
                }
            }
        }
    }
    known
}

fn reuse(m: &Module, f: &mut Function) {
    let entry = entry_known(f);
    let preds = f.predecessors();
    let nb = f.blocks.len();
    let mut claimed: Vec<Vec<bool>> = f
        .blocks
        .iter()
        .map(|b| vec![false; b.instrs.len()])
        .collect();
    // (block, drop index, alloc index) within one block.
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    // Drops of known constructors left without an allocation in their block:
    // (block, drop index, size).
    let mut open: Vec<(usize, usize, usize)> = Vec::new();
    for bi in 0..nb {
        let mut known: HashMap<Var, CtorRef> = HashMap::new();
        if let Some((v, c)) = entry[bi] {
            known.insert(v, c);
        }
        let instrs = &f.blocks[bi].instrs;
        for (i, ins) in instrs.iter().enumerate() {
            match ins {
                Instr::Let {
                    var: Some(_),
                    rhs: Rhs::Proj { obj, ctor, .. },
                } => {
                    known.insert(*obj, *ctor);
                }
                Instr::Let {
                    var: Some(v),
                    rhs: Rhs::Alloc { ctor, .. },
                } => {
                    known.insert(*v, *ctor);
                }
                Instr::Drop(x) => {
                    let Some(c) = known.get(x) else { continue };
                    let def = m.ctor(*c);
                    if def.fields.is_empty() {
                        continue;
                    }
                    let size = def.size();
                    match find_alloc(m, instrs, i + 1, size, &claimed[bi]) {
                        Some(j) => {
                            claimed[bi][j] = true;
                            pairs.push((bi, i, j));
                        }
                        None => open.push((bi, i, size)),
                    }
                }
                _ => {}
            }
        }
    }
    // A drop before a `switch` whose arms each allocate: the arms that
    // allocate a constructor of the same size reuse the token, the others
    // free it (`drop` of a token). Only arms entered from this block alone.
    let mut split: Vec<(usize, usize, Vec<(usize, Option<usize>)>)> = Vec::new();
    for (bi, i, size) in open {
        let Some(uses) = arm_uses(m, f, &preds, &claimed, bi, size, 0) else {
            continue;
        };
        if uses.iter().all(|(_, j)| j.is_none()) {
            continue;
        }
        for (t, j) in &uses {
            if let Some(j) = j {
                claimed[*t][*j] = true;
            }
        }
        split.push((bi, i, uses));
    }
    // Rewrite.
    for (bi, i, j) in pairs {
        let Instr::Drop(x) = f.blocks[bi].instrs[i] else {
            continue;
        };
        let tok = f.fresh_var("ru", Ty::Ptr);
        let b = &mut f.blocks[bi];
        b.instrs[i] = Instr::Let {
            var: Some(tok),
            rhs: Rhs::DropReuse(x),
        };
        set_reuse(&mut b.instrs[j], tok);
    }
    // Token drops are inserted last: they shift the indices of a block.
    let mut frees: Vec<(usize, Var)> = Vec::new();
    for (bi, i, uses) in split {
        let Instr::Drop(x) = f.blocks[bi].instrs[i] else {
            continue;
        };
        let tok = f.fresh_var("ru", Ty::Ptr);
        f.blocks[bi].instrs[i] = Instr::Let {
            var: Some(tok),
            rhs: Rhs::DropReuse(x),
        };
        for (t, j) in uses {
            match j {
                Some(j) => set_reuse(&mut f.blocks[t].instrs[j], tok),
                None => frees.push((t, tok)),
            }
        }
    }
    for (t, tok) in frees {
        f.blocks[t].instrs.insert(0, Instr::Drop(tok));
    }
}

/// Where the token of a drop at the end of block `b` goes: for each arm of
/// its `switch` (entered from `b` alone), the allocation of the same size
/// that reuses it, or `None` (the token is freed at the arm's start). An arm
/// without such an allocation that ends in a `switch` passes the token on
/// (up to a few levels). `None` when `b` does not end in a suitable switch.
fn arm_uses(
    m: &Module,
    f: &Function,
    preds: &[Vec<BlockId>],
    claimed: &[Vec<bool>],
    b: usize,
    size: usize,
    depth: u32,
) -> Option<Vec<(usize, Option<usize>)>> {
    let Terminator::Switch { arms, default, .. } = &f.blocks[b].term else {
        return None;
    };
    let mut targets: Vec<usize> = arms.iter().map(|a| a.1.0 as usize).collect();
    targets.extend(default.iter().map(|d| d.0 as usize));
    let n = targets.len();
    targets.sort();
    targets.dedup();
    if targets.len() != n || targets.iter().any(|t| preds[*t].len() != 1 || *t == b) {
        return None;
    }
    let mut uses = Vec::new();
    for t in targets {
        if let Some(j) = find_alloc(m, &f.blocks[t].instrs, 0, size, &claimed[t]) {
            uses.push((t, Some(j)));
            continue;
        }
        match (depth < 3)
            .then(|| arm_uses(m, f, preds, claimed, t, size, depth + 1))
            .flatten()
        {
            Some(inner) if inner.iter().any(|(_, j)| j.is_some()) => uses.extend(inner),
            _ => uses.push((t, None)),
        }
    }
    Some(uses)
}

/// The first unclaimed allocation of a non-nullary constructor of `size`
/// bytes in `instrs[from..]`.
fn find_alloc(
    m: &Module,
    instrs: &[Instr],
    from: usize,
    size: usize,
    claimed: &[bool],
) -> Option<usize> {
    instrs
        .iter()
        .enumerate()
        .skip(from)
        .find_map(|(j, ins)| match ins {
            Instr::Let {
                rhs: Rhs::Alloc {
                    ctor, reuse: None, ..
                },
                ..
            } if !claimed[j] => {
                let d = m.ctor(*ctor);
                (!d.fields.is_empty() && d.size() == size).then_some(j)
            }
            _ => None,
        })
}

fn set_reuse(ins: &mut Instr, tok: Var) {
    if let Instr::Let {
        rhs: Rhs::Alloc { reuse, .. },
        ..
    } = ins
    {
        *reuse = Some(tok);
    }
}

fn is_projection(f: &Function, v: Var) -> bool {
    f.blocks.iter().any(|b| {
        b.instrs.iter().any(|i| {
            matches!(i, Instr::Let { var: Some(x), rhs: Rhs::Proj { .. } } if *x == v)
                || matches!(i, Instr::Let { var: Some(x), rhs: Rhs::Prim(p, _) } if *x == v && p.result_borrowed())
        })
    })
}

fn fuse(sigs: &Sigs, f: &mut Function) {
    let nvars = f.var_names.len();
    let mut proj = vec![false; nvars];
    for (v, p) in proj.iter_mut().enumerate() {
        *p = is_projection(f, Var(v as u32));
    }
    for b in &mut f.blocks {
        let mut removed = vec![false; b.instrs.len()];
        for i in 0..b.instrs.len() {
            let Instr::Dup(x) = b.instrs[i] else { continue };
            if removed[i] {
                continue;
            }
            for j in i + 1..b.instrs.len() {
                if removed[j] {
                    continue;
                }
                match &b.instrs[j] {
                    Instr::Drop(y) if *y == x => {
                        removed[i] = true;
                        removed[j] = true;
                        break;
                    }
                    ins => {
                        // A projection may only be fused when adjacent.
                        if proj[x.0 as usize] {
                            break;
                        }
                        if instr_uses(sigs, ins)
                            .iter()
                            .any(|(v, m)| *v == x && *m == Mode::Consume)
                        {
                            break;
                        }
                    }
                }
            }
        }
        let mut k = 0;
        b.instrs.retain(|_| {
            k += 1;
            !removed[k - 1]
        });
    }
}

/// Moves `dup y`, where `y` is a projection of `x`, down to just before the
/// `drop x` / `drop_reuse x` that follows it in the same block, so that the
/// backend sees `dup y..; drop x` and can specialize the drop. A dup is only
/// moved past instructions that do not consume `y` (`y` stays valid through
/// `x` until the drop).
fn sink_dups(sigs: &Sigs, f: &mut Function) {
    let mut parent: HashMap<Var, Var> = HashMap::new();
    for b in &f.blocks {
        for i in &b.instrs {
            if let Instr::Let {
                var: Some(y),
                rhs: Rhs::Proj { obj, .. },
            } = i
            {
                parent.insert(*y, *obj);
            }
        }
    }
    for b in &mut f.blocks {
        let mut i = 0;
        while i < b.instrs.len() {
            let Instr::Dup(y) = b.instrs[i] else {
                i += 1;
                continue;
            };
            let Some(&x) = parent.get(&y) else {
                i += 1;
                continue;
            };
            // Find the drop of x.
            let mut target = None;
            for j in i + 1..b.instrs.len() {
                let ins = &b.instrs[j];
                let drops_x = matches!(ins, Instr::Drop(v) if *v == x)
                    || matches!(ins, Instr::Let { rhs: Rhs::DropReuse(v), .. } if *v == x);
                if drops_x {
                    target = Some(j);
                    break;
                }
                let uses = instr_uses(sigs, ins);
                if uses
                    .iter()
                    .any(|(v, m)| (*v == y || *v == x) && *m == Mode::Consume)
                {
                    break;
                }
            }
            match target {
                // Only dups lie in between: already in place (moving would
                // just swap dups forever).
                Some(j)
                    if b.instrs[i + 1..j]
                        .iter()
                        .all(|x| matches!(x, Instr::Dup(_))) =>
                {
                    i += 1
                }
                Some(j) if j > i + 1 => {
                    let d = b.instrs.remove(i);
                    b.instrs.insert(j - 1, d);
                    // Re-examine the instruction now at i.
                }
                _ => i += 1,
            }
        }
    }
}
