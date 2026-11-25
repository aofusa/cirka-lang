//! Product joins (FEAT-1009, 06-ir.md §1.7, ADR-114).
//!
//! A value of a product type (one constructor) that branches build and a
//! `case` takes apart right away travels as its fields instead:
//!
//! ```text
//! let! s = case c of 0 -> case grow$w$c b n k of UTup3 a m q -> MkBuilder a m q
//!                    _ -> MkBuilder b n k
//! in case s of MkBuilder buf len cap -> e
//! ==>
//! case (case c of 0 -> case grow$w$c b n k of UTup3 a m q -> UTup3 a m q
//!                 _ -> UTup3 b n k) of
//!   UTup3 buf len cap -> e
//! ```
//!
//! MIR lowers the branches as jumps to a join block whose parameters are
//! the fields: nothing is allocated (the builder of every `append` that
//! may grow its buffer), and `e` is not copied into each branch, which
//! case-of-case only does when `e` is small or every branch builds the
//! constructor itself. Every return must build the constructor (or
//! diverge): a branch returning a value that exists already (a variable,
//! a call) would have to take it apart, and its cell, which `e` may reuse
//! for a constructor of the same size (Perceus), would be freed instead.
//! A strict boxed field is evaluated where the constructor would have
//! evaluated it (a `let!` before the tuple).

use std::collections::BTreeSet;

use crate::cpr::{ensure_utup, is_utup, utup_name};
use crate::expr::*;
use crate::opt::{map_children, occurs};
use crate::ty::*;

/// Rewrites the product joins of `p` (see the module documentation) and
/// declares the unboxed tuples they use; the number rewritten.
pub fn product_joins(p: &mut Program, next: &mut u32) -> usize {
    let mut binds = std::mem::take(&mut p.binds);
    let mut j = Joiner {
        p,
        next: *next,
        count: 0,
        arities: BTreeSet::new(),
    };
    for b in &mut binds {
        let e = std::mem::replace(&mut b.expr, Expr::Lit(Lit::Unit));
        b.expr = j.expr(e);
    }
    let Joiner {
        next: n,
        count,
        arities,
        ..
    } = j;
    *next = n;
    p.binds = binds;
    for k in arities {
        ensure_utup(p, k);
    }
    count
}

struct Joiner<'p> {
    p: &'p Program,
    next: u32,
    count: usize,
    /// Sizes of the unboxed tuples used.
    arities: BTreeSet<usize>,
}

/// The product a join takes apart.
struct Fields<'a> {
    /// The binders of the alternative that takes it apart (the types and
    /// multiplicities of the fields).
    xs: &'a [Binder],
    /// The strictness of the constructor's fields.
    strict: &'a [bool],
    /// The unboxed tuple of the fields.
    ut: &'a Type,
}

impl Joiner<'_> {
    fn fresh(&mut self, name: &str) -> Id {
        let id = Id::new(name, self.next);
        self.next += 1;
        id
    }

    /// `e` with its product joins rewritten, innermost first.
    fn expr(&mut self, e: Expr) -> Expr {
        let e = map_children(e, &mut |c| self.expr(c));
        match e {
            // let! s = r in case s of K xs -> body
            Expr::LetStrict(s, r, k) => match *k {
                Expr::Case(sc, cb, res, alts)
                    if matches!(&*sc, Expr::Var(v) if v.id == s.id.id)
                        && self.joinable(&s.ty, &r, &cb, &alts, Some(s.id.id)) =>
                {
                    self.join(*r, res, alts)
                }
                k => Expr::LetStrict(s, r, Box::new(k)),
            },
            // case r of K xs -> body, where r branches
            Expr::Case(r, cb, res, alts)
                if matches!(&*r, Expr::Case(..) | Expr::Let(..) | Expr::LetStrict(..))
                    && self.joinable(&cb.ty, &r, &cb, &alts, None) =>
            {
                self.join(*r, res, alts)
            }
            e => e,
        }
    }

    /// Whether `case r as cb of alts` (with `r` bound to `s` first, if
    /// given) takes apart a product that every return of `r` builds: `ty`
    /// is a data type with one constructor of at least two fields, `alts`
    /// that constructor alone, the value itself is not used, and `r` has
    /// at least two returns, each building the constructor (or diverging).
    fn joinable(&self, ty: &Type, r: &Expr, cb: &Binder, alts: &[Alt], s: Option<u32>) -> bool {
        let [
            Alt {
                con: AltCon::Con(k, xs),
                rhs,
            },
        ] = alts
        else {
            return false;
        };
        let Type::Con(d, ts) = ty else { return false };
        let Some(dd) = self.p.data(d) else {
            return false;
        };
        if dd.newtype
            || is_utup(d)
            || dd.ctors.len() != 1
            || dd.ctors[0].name != *k
            || dd.params.len() != ts.len()
            || xs.len() < 2
        {
            return false;
        }
        if occurs(rhs, cb.id.id) || s.is_some_and(|x| occurs(rhs, x)) {
            return false;
        }
        let mut n = 0;
        returns(r, k, &mut n) && n >= 2
    }

    /// `case r' of UTup<n> xs -> body` for `case r as cb of K xs -> body`,
    /// where `r'` is `r` with each return turned into the tuple of its
    /// fields.
    fn join(&mut self, r: Expr, res: Type, alts: Vec<Alt>) -> Expr {
        let Some(Alt {
            con: AltCon::Con(k, xs),
            rhs,
        }) = alts.into_iter().next()
        else {
            unreachable!("checked by `joinable`")
        };
        let n = xs.len();
        let ut = Type::Con(utup_name(n), xs.iter().map(|x| x.ty.clone()).collect());
        let strict = self
            .p
            .ctor(&k)
            .map(|(d, i)| d.ctors[i].strict.clone())
            .unwrap_or_default();
        let f = Fields {
            xs: &xs,
            strict: &strict,
            ut: &ut,
        };
        let r2 = self.tuples(r, &f);
        let jb = Binder::new(self.fresh("j"), Mult::Many, ut.clone());
        self.count += 1;
        self.arities.insert(n);
        Expr::Case(
            Box::new(r2),
            jb,
            res,
            vec![Alt {
                con: AltCon::Con(utup_name(n), xs),
                rhs,
            }],
        )
    }

    /// `e` with each return replaced by the unboxed tuple of its fields.
    fn tuples(&mut self, e: Expr, f: &Fields) -> Expr {
        match e {
            Expr::Con(_, _, args) => {
                // A strict boxed field is evaluated before the tuple, as
                // the constructor would have evaluated it.
                let mut lets = Vec::new();
                let mut fields = Vec::with_capacity(args.len());
                for (i, a) in args.into_iter().enumerate() {
                    let x = &f.xs[i];
                    let strict = f.strict.get(i).copied().unwrap_or(false);
                    if strict
                        && x.mult != Mult::Zero
                        && !x.ty.is_unboxed()
                        && !matches!(a, Expr::Lit(_))
                    {
                        let v = Binder::new(self.fresh("f"), x.mult, x.ty.clone());
                        fields.push(Expr::var(&v.id));
                        lets.push((v, a));
                    } else {
                        fields.push(a);
                    }
                }
                let types = f.xs.iter().map(|x| x.ty.clone()).collect();
                let mut out = Expr::Con(utup_name(fields.len()), types, fields);
                for (v, a) in lets.into_iter().rev() {
                    out = Expr::LetStrict(v, Box::new(a), Box::new(out));
                }
                out
            }
            Expr::Panic(m, _) => Expr::Panic(m, f.ut.clone()),
            Expr::Let(b, body) => Expr::Let(b, Box::new(self.tuples(*body, f))),
            Expr::LetStrict(x, r, body) => Expr::LetStrict(x, r, Box::new(self.tuples(*body, f))),
            Expr::Case(s, b, _, alts) => {
                let alts = alts
                    .into_iter()
                    .map(|a| Alt {
                        con: a.con,
                        rhs: self.tuples(a.rhs, f),
                    })
                    .collect();
                Expr::Case(s, b, f.ut.clone(), alts)
            }
            _ => unreachable!("a return that does not build the constructor (`joinable`)"),
        }
    }
}

/// Whether every return of `e` builds constructor `k` or diverges; counts
/// the constructors in `n`.
fn returns(e: &Expr, k: &str, n: &mut usize) -> bool {
    match e {
        Expr::Con(c, _, _) => {
            *n += 1;
            c == k
        }
        Expr::Panic(..) => true,
        Expr::Let(_, b) | Expr::LetStrict(_, _, b) => returns(b, k, n),
        Expr::Case(_, _, _, alts) => alts.iter().all(|a| returns(&a.rhs, k, n)),
        _ => false,
    }
}
