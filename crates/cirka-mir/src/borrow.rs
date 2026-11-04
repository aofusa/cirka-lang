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
//! Fixed signatures (never changed): `main`, global initializers, the
//! non-captured parameters of closure functions, functions with hand-written
//! reference counting and every function they call.

use crate::ir::*;
use crate::perceus::{Kind, Mode, Sigs, VarInfo, has_rc, instr_uses, term_uses};
use std::collections::HashSet;

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
            break;
        }
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
