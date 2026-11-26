//! Exceptions (06-ir.md §2.5, FEAT-0040, ADR-124): which functions may
//! raise, and the owned references a function holds where an exception may
//! leave it, which the code generator's landing pads and the interpreter
//! drop on the way out.
//!
//! The cleanup sets come from the final MIR, by the verifier's data flow of
//! owned counts: after Perceus, reuse, fusion, the sinking of dups and the
//! specialization of loops, the liveness Perceus saw no longer tells what a
//! function holds, but the owned counts the verifier checks do.

use crate::ir::*;
use std::collections::HashMap;

/// Whether an instruction may raise, and from which ownership state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point {
    /// It cannot raise.
    No,
    /// It may raise after taking its operands (a call, `apply`, `force`,
    /// a runtime function reporting through the pending slot): the cleanup
    /// set is the state after the operands are consumed, before the result
    /// is defined.
    After,
    /// It may fail its check before doing anything (a bounds or zero
    /// check): the cleanup set is the state before the instruction.
    Before,
}

/// An owned reference held at a raising position: `count` references
/// through `var`, or a reuse token (`count` 1), freed without touching its
/// fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Held {
    pub var: Var,
    pub count: u32,
    pub token: bool,
}

/// The cleanup set of every raising position of a function, by (block,
/// instruction index). Positions with nothing to clean up are absent.
pub type Cleanups = HashMap<(usize, usize), Vec<Held>>;

/// Whether each function (by index in `m.funcs`) may raise: it raises or
/// panics, or it has a raising position other than a call of a function
/// that does not raise (a least fixpoint over the calls).
pub fn may_raise(m: &Module) -> Vec<bool> {
    let index = m.func_index();
    let n = m.funcs.len();
    let mut raises = vec![false; n];
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut work = Vec::new();
    for (fi, f) in m.funcs.iter().enumerate() {
        let mut direct = false;
        let mut callee = |g: &str| {
            if let Some(&gi) = index.get(g) {
                callers[gi].push(fi);
            }
        };
        for b in &f.blocks {
            for ins in &b.instrs {
                match ins {
                    Instr::Let {
                        rhs: Rhs::Call { func, .. },
                        ..
                    }
                    | Instr::LetMulti { func, .. } => callee(func),
                    _ if point(m, ins, &|_| false) != Point::No => direct = true,
                    _ => {}
                }
            }
            match &b.term {
                Terminator::TailCall { func, .. } => callee(func),
                Terminator::Raise(_) | Terminator::Panic(_) => direct = true,
                _ => {}
            }
        }
        if direct {
            raises[fi] = true;
            work.push(fi);
        }
    }
    while let Some(g) = work.pop() {
        for &f in &callers[g] {
            if !raises[f] {
                raises[f] = true;
                work.push(f);
            }
        }
    }
    raises
}

/// Whether `ins` (an instruction of `m`) may raise; `calls_raise` tells
/// whether a known function may (by name).
pub fn point(m: &Module, ins: &Instr, calls_raise: &dyn Fn(&str) -> bool) -> Point {
    match ins {
        Instr::Let { rhs, .. } => match rhs {
            Rhs::Call { func, .. } if calls_raise(func) => Point::After,
            Rhs::Apply { .. } | Rhs::Force(_) => Point::After,
            Rhs::Extern { func, .. } if m.extern_decl(func).is_some_and(|d| d.raises()) => {
                Point::After
            }
            Rhs::Prim(p, _) if fails(*p) => Point::Before,
            _ => Point::No,
        },
        Instr::LetMulti { func, .. } if calls_raise(func) => Point::After,
        // The exceptions of the application stop at the `catch`.
        Instr::Catch { .. } => Point::No,
        _ => Point::No,
    }
}

/// Whether a primitive checks its operands and fails with a catchable
/// error (an index out of bounds, a division by zero). The unchecked forms
/// only fail with an internal error.
pub fn fails(p: Prim) -> bool {
    p.unchecked().is_some()
}

/// The cleanup sets of `f` (a verified function of `m`), given which
/// functions may raise (`may_raise`).
pub fn cleanups(m: &Module, f: &Function, raises: &[bool]) -> Cleanups {
    let index = m.func_index();
    let calls_raise = |g: &str| index.get(g).is_some_and(|&i| raises[i]);
    let points: Vec<Vec<Point>> = f
        .blocks
        .iter()
        .map(|b| b.instrs.iter().map(|i| point(m, i, &calls_raise)).collect())
        .collect();
    crate::verify::owned_at(m, f, &points)
}
