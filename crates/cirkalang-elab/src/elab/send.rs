//! `Send` (spec-v0.0.5.md §1.1): whether values of a type can be given to
//! another thread. It has no implementations: the answer follows from the
//! structure of the type, like an auto trait of Rust.
//!
//! - Universes, functions (actions included) and the primitive scalars
//!   are `Send`; `ThreadBound` is not.
//! - A primitive type with type arguments is `Send` when they are.
//! - A data type is `Send` when the types of all the fields of all its
//!   constructors are, with the parameters replaced by the arguments.
//!   An index that a constructor's result gives as one of its fields
//!   (`VCons : a -> Vect n a -> Vect (S n) a` gives `a`) is replaced by the
//!   argument too; the other fields stand for unknown values. A recursive
//!   occurrence with the same arguments (unknown values aside) is assumed
//!   `Send` (greatest fixed point).
//! - A type variable of the context is `Send` when a constraint says so.
//! - A field whose type is unknown (existential) is not `Send`, unless it
//!   is erased (multiplicity 0): an erased value is not there at run time.

use super::traits::Search;
use super::*;

/// Fields of constructors that the result type does not determine:
/// unknown values. Context levels are always smaller.
const UNKNOWN: u32 = 1 << 29;

/// Fields while the result type of a constructor is matched: field `k` is
/// the variable at level `PROBE + k`.
const PROBE: u32 = 1 << 30;

/// Recursive occurrences of a data type with other arguments, at most:
/// a type that grows its arguments (`Nest a` holding `Nest (List a)`) is
/// not checked further.
const MAX_NESTING: usize = 16;

/// Why a type is not `Send`.
pub(super) enum SendNo {
    /// The answer depends on a meta that is not solved yet.
    Stuck,
    No {
        /// The part that is not `Send`.
        why: String,
        /// The fields that lead to it, innermost first.
        path: Vec<String>,
        /// A type variable that lacks a `Send` constraint (for the help).
        var: Option<String>,
    },
}

fn no(why: impl Into<String>) -> Result<(), SendNo> {
    Err(SendNo::No {
        why: why.into(),
        path: Vec::new(),
        var: None,
    })
}

impl<'r> Elab<'r> {
    /// Whether `ty` (in `cx`) is `Send`.
    pub(super) fn send_check(&self, cx: &Cxt, ty: &Val) -> Result<(), SendNo> {
        self.send_go(cx, ty, &mut Vec::new())
    }

    fn send_go(
        &self,
        cx: &Cxt,
        ty: &Val,
        visiting: &mut Vec<(GName, Vec<Val>)>,
    ) -> Result<(), SendNo> {
        let w = self.whnf(ty);
        match &w {
            Val::Neutral(Head::Meta(_), _) => Err(SendNo::Stuck),
            Val::Neutral(Head::Var(l), _) if *l >= UNKNOWN => {
                no("the type of a field is not known from the type (an existential type)")
            }
            Val::Neutral(Head::Var(_), _) => self.send_var(cx, &w),
            Val::Neutral(Head::Global(g), sp) => match self.prog.prim_of(*g) {
                Some(Prim::ThreadBound) => {
                    no("`ThreadBound` marks a value that only its thread may use")
                }
                // Actions are functions.
                Some(Prim::IO | Prim::IO1 | Prim::RIO | Prim::Borrow | Prim::BorrowIO) => Ok(()),
                Some(_) => {
                    let mut k = self.global_type(*g).unwrap_or(Val::Error);
                    let mut stuck = false;
                    for e in sp {
                        let Elim::App(a, _) = e else {
                            return no(format!("`{}` cannot be checked", self.show_send(cx, &w)));
                        };
                        let Val::Pi(_, _, _, dom, c) = self.whnf(&k) else {
                            break;
                        };
                        if matches!(self.whnf(&dom), Val::Type | Val::LType) {
                            match self.send_go(cx, a, visiting) {
                                Ok(()) => {}
                                Err(SendNo::Stuck) => stuck = true,
                                Err(e) => return Err(e),
                            }
                        }
                        k = self.ev().inst(&c, a.clone());
                    }
                    if stuck { Err(SendNo::Stuck) } else { Ok(()) }
                }
                // An opaque definition or an `extern` function.
                None => no(format!("`{}` cannot be checked", self.show_send(cx, &w))),
            },
            Val::Data(d, args) => self.send_data(cx, *d, args, visiting),
            Val::Type
            | Val::LType
            | Val::Pi(..)
            | Val::Error
            | Val::Con(..)
            | Val::Lam(..)
            | Val::Lit(_) => Ok(()),
        }
    }

    /// A type variable of the context (maybe applied): `Send` from a
    /// constraint in scope.
    fn send_var(&self, cx: &Cxt, t: &Val) -> Result<(), SendNo> {
        let Some(tr) = self.prog.builtins.send_trait else {
            return Ok(());
        };
        let gargs = vec![(t.clone(), Icit::Expl)];
        let mut stuck = false;
        for l in (0..cx.lvl()).rev() {
            if cx.mults[l as usize] == Mult::Zero {
                continue;
            }
            let lt = self.ty_val(cx, l);
            match self.local_candidate(cx, &Term::Var(cx.ix(l)), &lt, tr, &gargs) {
                Search::Found(_) => return Ok(()),
                Search::Stuck => stuck = true,
                Search::NotFound => {}
            }
        }
        if stuck {
            return Err(SendNo::Stuck);
        }
        let name = self.show_send(cx, t);
        Err(SendNo::No {
            why: format!("the type variable `{name}` has no `Send` constraint"),
            path: Vec::new(),
            var: Some(name),
        })
    }

    fn send_data(
        &self,
        cx: &Cxt,
        d: GName,
        args: &[(Val, Icit)],
        visiting: &mut Vec<(GName, Vec<Val>)>,
    ) -> Result<(), SendNo> {
        let argv: Vec<Val> = args.iter().map(|a| a.0.clone()).collect();
        let mut nested = 0;
        for (vd, vargs) in visiting.iter() {
            if *vd == d {
                if self.same_args(cx.lvl(), vargs, &argv) {
                    return Ok(());
                }
                nested += 1;
            }
        }
        if nested >= MAX_NESTING {
            return no(format!(
                "`{}` nests itself with ever larger arguments",
                self.prog.short_name(d)
            ));
        }
        let Global::Data { cons, nparams, .. } = self.prog.get(d).clone() else {
            return Ok(());
        };
        visiting.push((d, argv));
        let mut res = Ok(());
        'cons: for c in cons {
            let Some(cty) = self.prog.type_of(c) else {
                continue;
            };
            let base = self.ev().eval(&Vec::new(), cty);
            let mut v = base.clone();
            for (a, _) in args.iter().take(nparams) {
                match self.whnf(&v) {
                    Val::Pi(_, _, _, _, clo) => v = self.ev().inst(&clo, a.clone()),
                    _ => break,
                }
            }
            let given = self.given_fields(&v, &args[nparams.min(args.len())..]);
            let mut k = 0;
            let mut expl = 0;
            while let Val::Pi(x, m, i, a, clo) = self.whnf(&v) {
                if i == Icit::Expl {
                    expl += 1;
                }
                let erased_unknown = m == Mult::Zero
                    && matches!(self.whnf(&a), Val::Neutral(Head::Var(l), _) if l >= UNKNOWN);
                if !erased_unknown {
                    match self.send_go(cx, &a, visiting) {
                        Ok(()) => {}
                        Err(SendNo::Stuck) => res = Err(SendNo::Stuck),
                        Err(SendNo::No { why, mut path, var }) => {
                            let con = self.prog.short_name(c);
                            path.push(if i == Icit::Expl {
                                format!("field {expl} of `{con}`")
                            } else {
                                format!("the implicit field `{x}` of `{con}`")
                            });
                            res = Err(SendNo::No { why, path, var });
                            break 'cons;
                        }
                    }
                }
                let fv = given
                    .get(k)
                    .cloned()
                    .flatten()
                    .unwrap_or_else(|| Val::var(UNKNOWN));
                v = self.ev().inst(&clo, fv);
                k += 1;
            }
        }
        visiting.pop();
        res
    }

    /// The fields of a constructor (of type `v`, parameters instantiated)
    /// that its result type gives as an index, with the argument at that
    /// index: `VCons : {n} -> {a} -> a -> Vect n a -> Vect (S n) a` against
    /// the indices `[m, Int]` gives `a := Int`.
    fn given_fields(&self, v: &Val, indices: &[(Val, Icit)]) -> Vec<Option<Val>> {
        let mut v = v.clone();
        let mut n = 0u32;
        while let Val::Pi(_, _, _, _, clo) = self.whnf(&v) {
            v = self.ev().inst(&clo, Val::var(PROBE + n));
            n += 1;
        }
        let mut given = vec![None; n as usize];
        let Val::Data(_, rargs) = self.whnf(&v) else {
            return given;
        };
        let nres = rargs.len();
        let first_index = nres.saturating_sub(indices.len());
        for (j, (r, _)) in rargs.iter().enumerate().skip(first_index) {
            if let Val::Neutral(Head::Var(l), sp) = self.whnf(r)
                && sp.is_empty()
                && l >= PROBE
                && ((l - PROBE) as usize) < given.len()
                && given[(l - PROBE) as usize].is_none()
            {
                given[(l - PROBE) as usize] = Some(indices[j - first_index].0.clone());
            }
        }
        given
    }

    /// The arguments of two occurrences of a data type are the same, unknown
    /// values aside (all unknown values are alike: the check treats them the
    /// same way). `false` when unsure.
    fn same_args(&self, lvl: u32, a: &[Val], b: &[Val]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| self.same_val(lvl, x, y))
    }

    fn same_val(&self, lvl: u32, a: &Val, b: &Val) -> bool {
        let a = self.whnf(a);
        let b = self.whnf(b);
        let unknown = |v: &Val| matches!(v, Val::Neutral(Head::Var(l), _) if *l >= UNKNOWN);
        if unknown(&a) && unknown(&b) {
            return true;
        }
        match (&a, &b) {
            (Val::Neutral(h1, s1), Val::Neutral(h2, s2)) => {
                h1 == h2
                    && s1.len() == s2.len()
                    && s1.iter().zip(s2).all(|(x, y)| match (x, y) {
                        (Elim::App(p, i), Elim::App(q, j)) => i == j && self.same_val(lvl, p, q),
                        _ => false,
                    })
            }
            (Val::Data(d1, a1), Val::Data(d2, a2)) => {
                d1 == d2
                    && a1.len() == a2.len()
                    && a1
                        .iter()
                        .zip(a2)
                        .all(|(x, y)| self.same_val(lvl, &x.0, &y.0))
            }
            (Val::Con(c1, a1), Val::Con(c2, a2)) => {
                c1 == c2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|(x, y)| self.same_val(lvl, x, y))
            }
            (Val::Pi(_, m1, i1, a1, c1), Val::Pi(_, m2, i2, a2, c2)) => {
                let x = Val::var(lvl);
                m1 == m2
                    && i1 == i2
                    && self.same_val(lvl, a1, a2)
                    && self.same_val(
                        lvl + 1,
                        &self.ev().inst(c1, x.clone()),
                        &self.ev().inst(c2, x),
                    )
            }
            (Val::Lam(_, _, i1, _, c1), Val::Lam(_, _, i2, _, c2)) => {
                let x = Val::var(lvl);
                i1 == i2
                    && self.same_val(
                        lvl + 1,
                        &self.ev().inst(c1, x.clone()),
                        &self.ev().inst(c2, x),
                    )
            }
            (Val::Type, Val::Type) | (Val::LType, Val::LType) | (Val::Error, Val::Error) => true,
            (Val::Lit(x), Val::Lit(y)) => x == y,
            _ => false,
        }
    }

    /// A type for a message. Unknown values (which quote outside the
    /// context) are not shown.
    fn show_send(&self, cx: &Cxt, v: &Val) -> String {
        if self.mentions_unknown(v) {
            return "a type of a field".into();
        }
        self.show(cx, v)
    }

    fn mentions_unknown(&self, v: &Val) -> bool {
        match self.whnf(v) {
            Val::Neutral(h, sp) => {
                matches!(h, Head::Var(l) if l >= UNKNOWN)
                    || sp.iter().any(|e| match e {
                        Elim::App(a, _) => self.mentions_unknown(a),
                        Elim::Case(..) => false,
                    })
            }
            Val::Data(_, a) => a.iter().any(|x| self.mentions_unknown(&x.0)),
            Val::Con(_, a) => a.iter().any(|x| self.mentions_unknown(x)),
            _ => false,
        }
    }

    /// The dictionary of `Send t` (it has no fields).
    pub(super) fn send_dict(&self, tr: GName, t: &Term) -> Term {
        let con = self.prog.traits.get(&tr).map(|i| i.con).unwrap_or(tr);
        Term::App(Rc::new(Term::Global(con)), Rc::new(t.clone()), Icit::Impl)
    }

    /// E1261 for an unsolvable `Send t`.
    pub(super) fn send_error(&self, cx: &Cxt, t: &Val, span: Span) -> Diagnostic {
        let mut d = self.error(
            "E1261",
            span,
            format!(
                "`{}` cannot be sent to another thread (it is not `Send`)",
                self.show(cx, t)
            ),
        );
        if let Err(SendNo::No { why, path, var }) = self.send_check(cx, t) {
            // The fields from the innermost one (a few: a type that nests
            // itself gives long paths).
            let mut shown: Vec<&str> = path.iter().map(String::as_str).take(3).collect();
            if path.len() > shown.len() {
                shown.push("…");
            }
            let note = if shown.is_empty() {
                why
            } else {
                format!("{why}: {}", shown.join(", in "))
            };
            d = d.with_note(note);
            if let Some(v) = var {
                d = d.with_help(format!("add the constraint `Send {v}` to the signature"));
            }
        }
        d
    }
}
