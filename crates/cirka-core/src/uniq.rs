//! Removal of redundant uniqueness checks (FEAT-0063).
//!
//! In-place updates of arrays go through `bytes_unique` / `array_unique`,
//! which return the array itself when it is unique and a copy otherwise.
//! A loop body that writes several elements checks each time:
//!
//! ```text
//! let! c1 = bytes_unique a in … bytes_setf c1 i x … let! c2 = bytes_unique c1 in …
//! ```
//!
//! The result of a check is unique, and it stays unique while it is only
//! read and written in place (the element operations do not keep a
//! reference), so the second check returns `c1` itself: `c2` becomes `c1`.
//! This leaves one check (an inline load and compare) per loop iteration.

use crate::expr::*;
use crate::opt::{map_children, replace_var, walk};

/// Collapses the redundant checks of every binding; returns how many.
pub fn redundant_unique(p: &mut Program) -> usize {
    let mut n = 0;
    for b in &mut p.binds {
        let e = std::mem::replace(&mut b.expr, Expr::Lit(Lit::Unit));
        b.expr = go(e, &mut n);
    }
    n
}

fn is_check(op: &PrimOp) -> bool {
    matches!(op, PrimOp::BytesUnique | PrimOp::ArrayUnique)
}

/// Operations that use their first argument (the array) without keeping a
/// reference to it.
fn in_place(op: &PrimOp) -> bool {
    use PrimOp::*;
    matches!(
        op,
        BytesLen
            | BytesGet
            | BytesSet
            | BytesGet64
            | BytesSet64
            | BytesGetF
            | BytesSetF
            | ArrayLen
            | ArrayGet
            | ArraySet
    )
}

/// `let! c = x` or `let! c = check x`: `c` is `x` once `x` is known unique.
fn same_as(rhs: &Expr, x: u32) -> bool {
    match rhs {
        Expr::Var(v) => v.id == x,
        Expr::Prim(op, _, a) => is_check(op) && matches!(a.as_slice(), [Expr::Var(v)] if v.id == x),
        _ => false,
    }
}

/// Whether `rhs` makes a value nothing else refers to.
fn fresh(rhs: &Expr) -> bool {
    matches!(rhs, Expr::Prim(op, _, _)
        if is_check(op) || matches!(op, PrimOp::BytesNew | PrimOp::ArrayNew))
}

fn go(e: Expr, n: &mut usize) -> Expr {
    let e = map_children(e, &mut |c| go(c, n));
    match e {
        Expr::LetStrict(b, rhs, body) if fresh(&rhs) => {
            let (body, _) = collapse(*body, b.id.id, &Expr::var(&b.id), n);
            Expr::LetStrict(b, rhs, Box::new(body))
        }
        e => e,
    }
}

fn occurs(e: &Expr, x: u32) -> bool {
    let mut found = false;
    walk(e, &mut |s| found |= matches!(s, Expr::Var(v) if v.id == x));
    found
}

/// Replaces `let! c = x` / `let! c = check x` by `c := x` while `x` (unique
/// when `e` starts) has only been used in place, in evaluation order. The
/// flag says whether `x` is still unique after `e`.
fn collapse(e: Expr, x: u32, xv: &Expr, n: &mut usize) -> (Expr, bool) {
    if !occurs(&e, x) {
        return (e, true);
    }
    match e {
        // The new name must be the only one used afterwards: if `x` were
        // still used, the check would have copied.
        Expr::LetStrict(c, rhs, k) if same_as(&rhs, x) && !occurs(&k, x) => {
            *n += 1;
            let k = replace_var(*k, c.id.id, xv);
            collapse(k, x, xv, n)
        }
        Expr::LetStrict(c, rhs, k) => {
            let (rhs, ok) = collapse(*rhs, x, xv, n);
            if !ok {
                return (Expr::LetStrict(c, Box::new(rhs), k), false);
            }
            let (k, ok) = collapse(*k, x, xv, n);
            (Expr::LetStrict(c, Box::new(rhs), Box::new(k)), ok)
        }
        Expr::Prim(op, ts, a) => {
            let skip =
                usize::from(in_place(&op) && matches!(a.first(), Some(Expr::Var(v)) if v.id == x));
            let (a, ok) = seq(a, skip, x, xv, n);
            (Expr::Prim(op, ts, a), ok)
        }
        Expr::Con(c, ts, a) => {
            let (a, ok) = seq(a, 0, x, xv, n);
            (Expr::Con(c, ts, a), ok)
        }
        Expr::Extern(f, a) => {
            let (a, ok) = seq(a, 0, x, xv, n);
            (Expr::Extern(f, a), ok)
        }
        Expr::Cast(s, t) => {
            let (s, ok) = collapse(*s, x, xv, n);
            (Expr::Cast(Box::new(s), t), ok)
        }
        Expr::TyApp(s, t) => {
            let (s, ok) = collapse(*s, x, xv, n);
            (Expr::TyApp(Box::new(s), t), ok)
        }
        Expr::Case(s, b, t, alts) => {
            let (s, ok) = collapse(*s, x, xv, n);
            if !ok {
                return (Expr::Case(Box::new(s), b, t, alts), false);
            }
            let mut all = true;
            let alts = alts
                .into_iter()
                .map(|a| {
                    let (rhs, ok) = collapse(a.rhs, x, xv, n);
                    all &= ok;
                    Alt { con: a.con, rhs }
                })
                .collect();
            (Expr::Case(Box::new(s), b, t, alts), all)
        }
        // Any other use (a variable, an argument of a call, a capture by a
        // lambda or a thunk) may share `x`.
        e => (e, false),
    }
}

/// Arguments evaluated in order; the first `skip` are in-place uses of `x`.
fn seq(a: Vec<Expr>, skip: usize, x: u32, xv: &Expr, n: &mut usize) -> (Vec<Expr>, bool) {
    let mut ok = true;
    let a = a
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            if i < skip || !ok {
                return s;
            }
            let (s, k) = collapse(s, x, xv, n);
            ok = k;
            s
        })
        .collect();
    (a, ok)
}
