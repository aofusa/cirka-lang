//! Refinement types (spec-v0.0.4.md §3): every value checked against
//! `{x : A | p}` (marked `primRefine` by elaboration) must satisfy `p`.
//! An obligation is discharged by evaluation when `p` reduces to `True`,
//! and otherwise by Z3, under what the path to the value has learnt (the
//! matched constructors and literals, the conditions of `if`), the
//! refinements of the binders in scope and of the results of the
//! functions it calls.

use crate::Contracts;
use crate::measure::{Tr, smt_names};
use crate::walk::{Assume, Facts, Obligation};
use cirka_diagnostics::Diagnostic;
use cirka_smt::{Expr, Outcome, Query, Solver};
use cirkalang_elab::eval::Ev;
use cirkalang_elab::tt::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Does the term mark a value checked against a refinement?
pub fn mentions_refine(t: &Term, refine: GName) -> bool {
    match t {
        Term::Global(g) => *g == refine,
        Term::App(f, a, _) => mentions_refine(f, refine) || mentions_refine(a, refine),
        Term::Lam(_, _, _, _, b) => mentions_refine(b, refine),
        Term::Let(_, _, _, v, b) => mentions_refine(v, refine) || mentions_refine(b, refine),
        Term::Case(c) => {
            mentions_refine(&c.scrut, refine)
                || c.alts.iter().any(|a| match a {
                    Alt::Con { body, .. } | Alt::Lit { body, .. } => mentions_refine(body, refine),
                })
                || c.default
                    .as_ref()
                    .is_some_and(|d| mentions_refine(d, refine))
        }
        _ => false,
    }
}

/// Checks the obligations of one definition. With `--contracts=check`,
/// those that are not proved (but not known to be false) are checked at
/// run time instead: they are added to `unproved` with a warning.
pub fn check(
    ev: &Ev,
    solver: Option<&Solver>,
    prog: &Program,
    facts: &Facts,
    mode: Contracts,
    diags: &mut Vec<Diagnostic>,
    unproved: &mut Vec<(usize, String)>,
) {
    let names = smt_names(facts);
    for ob in &facts.obligations {
        let Some(site) = prog.refine_sites.get(ob.site) else {
            continue;
        };
        let Some(d) = obligation(ev, solver, prog, facts, &names, ob) else {
            continue;
        };
        let refuted = d.message.contains("does not hold");
        // Undecided (W0503): checked at run time even with `verify`, so that
        // what relies on the refinement stays safe.
        let undecided = !d.is_error();
        if (mode == Contracts::Check && !refuted) || undecided {
            if undecided {
                diags.push(d.with_label(site.span, format!("in `{}`", site.def)));
                let shown = show(
                    ev,
                    facts,
                    &ev.apply(ob.pred.clone(), ob.val.clone(), Icit::Expl),
                );
                unproved.push((
                    ob.site,
                    format!("contract violated in `{}`: {shown}", site.def),
                ));
                continue;
            }
            let what = d.message.clone();
            let shown = show(
                ev,
                facts,
                &ev.apply(ob.pred.clone(), ob.val.clone(), Icit::Expl),
            );
            unproved.push((
                ob.site,
                format!("contract violated in `{}`: {shown}", site.def),
            ));
            diags.push(
                Diagnostic::warning(format!("checked at run time: {what}"))
                    .with_code("W0507")
                    .with_label(site.span, format!("in `{}`", site.def)),
            );
            continue;
        }
        diags.push(d.with_label(site.span, format!("in `{}`", site.def)));
    }
}

/// Replaces `primRefine {a} p site e` for the given sites by
/// `checkContract {a} p msg e`.
pub fn insert(
    t: &Term,
    refine: GName,
    chk: GName,
    bool_ty: GName,
    msgs: &HashMap<usize, String>,
) -> Term {
    let go = |t: &Term| insert(t, refine, chk, bool_ty, msgs);
    match t {
        Term::App(f, a, i) => {
            // `primRefine {a} p site e` (heads first).
            if let Term::App(f2, site, _) = f.as_ref()
                && let Term::App(f3, _, _) = f2.as_ref()
                && let Term::App(h, _, _) = f3.as_ref()
                && matches!(h.as_ref(), Term::Global(g) if *g == refine)
                && let Term::Lit(Lit::Int(k)) = site.as_ref()
                && let Some(m) = msgs.get(&(*k as usize))
            {
                let Term::App(f3, p, pi) = f2.as_ref() else {
                    unreachable!()
                };
                let Term::App(_, ty, ti) = f3.as_ref() else {
                    unreachable!()
                };
                let app = |f: Term, x: Term, i: Icit| Term::App(Rc::new(f), Rc::new(x), i);
                let h = app(Term::Global(chk), ty.as_ref().clone(), *ti);
                let h = app(h, bind_scruts(&go(p), bool_ty), *pi);
                let h = app(h, Term::Lit(Lit::Str(Rc::from(m.as_str()))), Icit::Expl);
                return app(h, go(a), *i);
            }
            Term::App(Rc::new(go(f)), Rc::new(go(a)), *i)
        }
        Term::Lam(x, m, i, ty, b) => Term::Lam(x.clone(), *m, *i, ty.clone(), Rc::new(go(b))),
        Term::Let(x, m, ty, v, b) => {
            Term::Let(x.clone(), *m, ty.clone(), Rc::new(go(v)), Rc::new(go(b)))
        }
        Term::Case(c) => {
            let mut c2 = c.as_ref().clone();
            c2.scrut = go(&c.scrut);
            for a in &mut c2.alts {
                match a {
                    Alt::Con { body, .. } | Alt::Lit { body, .. } => *body = go(body),
                }
            }
            c2.default = c.default.as_ref().map(go);
            Term::Case(Rc::new(c2))
        }
        _ => t.clone(),
    }
}

fn obligation(
    ev: &Ev,
    solver: Option<&Solver>,
    prog: &Program,
    facts: &Facts,
    names: &std::collections::HashMap<u32, String>,
    ob: &Obligation,
) -> Option<Diagnostic> {
    let holds = ev.apply(ob.pred.clone(), ob.val.clone(), Icit::Expl);
    let shown = show(ev, facts, &holds);
    // Decided by evaluation.
    ev.fuel.set(100_000);
    match ev.whnf(holds.clone()) {
        Val::Con(c, _) if Some(c) == prog.builtins.true_ => return None,
        Val::Con(c, _) if Some(c) == prog.builtins.false_ && ob.conds.is_empty() => {
            return Some(
                Diagnostic::error(format!("the refinement `{shown}` does not hold"))
                    .with_code("E0501")
                    .with_note("it evaluates to `False`"),
            );
        }
        _ => {}
    }
    let Some(solver) = solver else {
        return Some(
            Diagnostic::error(format!("cannot verify the refinement `{shown}`"))
                .with_code("E0501")
                .with_note("it needs Z3, which was not found (set `CIRKA_Z3`)"),
        );
    };
    let mut tr = Tr::new(ev, &facts.sorts, names);
    let goal = tr.boolean(&holds, 0);
    // What the refined results of the calls in the value tell.
    let mut calls = Vec::new();
    refined_calls(ev, prog, &ob.val, &mut calls, 0);
    for h in calls {
        let e = tr.boolean(&h, 0);
        tr.extra.push(e);
    }
    let mut hyps: Vec<Expr> = ob.conds.iter().filter_map(|c| tr.cond(c)).collect();
    for a in &ob.assumed {
        match a {
            Assume::Holds(h) => hyps.push(tr.boolean(h, 0)),
            Assume::Fact(Val::Data(g, args))
                if Some(*g) == prog.builtins.equal && args.len() == 3 =>
            {
                let a = ev.whnf(args[0].0.clone());
                let bool_ty = matches!(a, Val::Data(d, _) if Some(d) == prog.builtins.bool_);
                if bool_ty {
                    let (x, y) = (tr.boolean(&args[1].0, 0), tr.boolean(&args[2].0, 0));
                    hyps.push(Expr::eq(x, y));
                } else {
                    let (x, y) = (tr.int(&args[1].0, 0), tr.int(&args[2].0, 0));
                    hyps.push(Expr::eq(x, y));
                }
            }
            Assume::Fact(Val::Data(_, args)) if args.len() == 1 => {
                hyps.push(tr.boolean(&args[0].0, 0));
            }
            Assume::Fact(_) => {}
        }
    }
    // The hypotheses learnt while translating (refined results, `Nat`s).
    hyps.extend(tr.var_hyps());
    let q = Query {
        hyps,
        goal: Some(goal),
    };
    match solver.prove(&q) {
        Ok(Outcome::Valid) => None,
        Ok(Outcome::Invalid(model)) => {
            let vs: Vec<String> = model
                .iter()
                .filter_map(|(k, v)| {
                    let name = match tr.opaque_vals.get(k) {
                        Some(val) => {
                            let s = show(ev, facts, val);
                            // Unfolded definitions are not worth showing.
                            if s.contains("case ") || s.len() > 40 {
                                return None;
                            }
                            s
                        }
                        None => k.split('$').next().unwrap_or(k).to_string(),
                    };
                    Some(format!("{name} = {v}"))
                })
                .collect();
            let mut d = Diagnostic::error(format!("cannot prove the refinement `{shown}`"))
                .with_code("E0501");
            if !vs.is_empty() {
                d = d.with_note(format!("counterexample: {}", vs.join(", ")));
            }
            Some(d.with_help(
                "check the value, add a condition (`if`), or refine the type of what it is computed from",
            ))
        }
        Ok(Outcome::Unknown(why)) | Err(why) => Some(
            Diagnostic::warning(format!("the refinement `{shown}` was not decided"))
                .with_code("W0503")
                .with_note(why)
                .with_help("simplify the refinement, or raise `CIRKA_VERIFY_TIMEOUT`"),
        ),
    }
}

/// The obligation as source-like text, with the variables' names.
fn show(ev: &Ev, facts: &Facts, v: &Val) -> String {
    let lvl = facts.names.keys().max().map_or(0, |m| m + 1);
    let t = ev.quote(lvl, v);
    let mut names: Vec<Name> = (0..lvl)
        .map(|l| {
            let n = facts
                .names
                .get(&l)
                .cloned()
                .unwrap_or_else(|| format!("v{l}"));
            Name::from(n.as_str())
        })
        .collect();
    pretty(ev.prog, &mut names, &t, false)
}

/// Prints operators infix (`n + 1 > n`); falls back to the TT printer.
fn pretty(prog: &Program, names: &mut Vec<Name>, t: &Term, nested: bool) -> String {
    let mut args = Vec::new();
    let mut h = t;
    while let Term::App(f, a, i) = h {
        if *i == Icit::Expl {
            args.push(a.as_ref());
        }
        h = f;
    }
    args.reverse();
    if let (Term::Case(c), true) = (h, args.is_empty())
        && let Some(s) = bool_case(prog, names, c, nested)
    {
        return s;
    }
    let Term::Global(g) = h else {
        return print_term(prog, names, t);
    };
    if args.is_empty() {
        return print_term(prog, names, t);
    }
    let name = match prog.get(*g) {
        Global::Prim { prim, .. } => prim_op(*prim).unwrap_or(prog.short_name(*g)),
        _ => prog.short_name(*g),
    };
    let op = !name.starts_with(|c: char| c.is_alphanumeric() || c == '_');
    let out = if op && args.len() == 2 {
        format!(
            "{} {name} {}",
            pretty(prog, names, args[0], true),
            pretty(prog, names, args[1], true)
        )
    } else {
        let mut s = if op {
            format!("({name})")
        } else {
            name.to_string()
        };
        for a in args {
            s.push(' ');
            s.push_str(&pretty(prog, names, a, true));
        }
        s
    };
    if nested { format!("({out})") } else { out }
}

fn prim_op(p: Prim) -> Option<&'static str> {
    Some(match p {
        Prim::AddInt => "+",
        Prim::SubInt => "-",
        Prim::MulInt => "*",
        Prim::DivInt => "/",
        Prim::ModInt => "%",
        Prim::EqInt => "==",
        Prim::LtInt => "<",
        Prim::LeInt => "<=",
        _ => return None,
    })
}

/// A match on a boolean: `a && b`, `a || b`, `if c then t else f`.
fn bool_case(prog: &Program, names: &mut Vec<Name>, c: &Case, nested: bool) -> Option<String> {
    let b = &prog.builtins;
    let (t, f) = (b.true_?, b.false_?);
    let mut on_true = None;
    let mut on_false = None;
    for a in &c.alts {
        match a {
            Alt::Con { con, body, .. } if *con == t => on_true = Some(body),
            Alt::Con { con, body, .. } if *con == f => on_false = Some(body),
            _ => return None,
        }
    }
    let on_true = on_true.or(c.default.as_ref())?;
    let on_false = on_false.or(c.default.as_ref())?;
    let is = |x: &Term, k: GName| matches!(x, Term::Global(g) if *g == k);
    let s = pretty(prog, names, &c.scrut, true);
    let out = if is(on_false, f) {
        format!("{s} && {}", pretty(prog, names, on_true, true))
    } else if is(on_true, t) {
        format!("{s} || {}", pretty(prog, names, on_false, true))
    } else {
        format!(
            "if {s} then {} else {}",
            pretty(prog, names, on_true, false),
            pretty(prog, names, on_false, false)
        )
    };
    Some(if nested { format!("({out})") } else { out })
}

/// A predicate taken from a type may match on an expression (`a && b`
/// is a match on `a`); lowering matches on variables: `case e of alts`
/// becomes `let c : Bool = e in case c of alts` (for booleans).
fn bind_scruts(t: &Term, bool_ty: GName) -> Term {
    let go = |t: &Term| Rc::new(bind_scruts(t, bool_ty));
    match t {
        Term::App(f, a, i) => Term::App(go(f), go(a), *i),
        Term::Lam(x, m, i, ty, b) => Term::Lam(x.clone(), *m, *i, ty.clone(), go(b)),
        Term::Let(x, m, ty, v, b) => Term::Let(x.clone(), *m, ty.clone(), go(v), go(b)),
        Term::Case(c) => {
            let mut c2 = c.as_ref().clone();
            c2.scrut = bind_scruts(&c.scrut, bool_ty);
            for a in &mut c2.alts {
                match a {
                    Alt::Con { body, .. } | Alt::Lit { body, .. } => {
                        *body = bind_scruts(body, bool_ty)
                    }
                }
            }
            c2.default = c.default.as_ref().map(|d| bind_scruts(d, bool_ty));
            if matches!(c2.scrut, Term::Var(_)) {
                return Term::Case(Rc::new(c2));
            }
            let scrut = std::mem::replace(&mut c2.scrut, Term::Var(0));
            let Term::Case(shifted) = shift(&Term::Case(Rc::new(c2)), 0, 1) else {
                unreachable!()
            };
            let mut c3 = shifted.as_ref().clone();
            c3.scrut = Term::Var(0);
            Term::Let(
                Name::from("c"),
                Mult::Many,
                Rc::new(Term::Global(bool_ty)),
                Rc::new(scrut),
                Rc::new(Term::Case(Rc::new(c3))),
            )
        }
        _ => t.clone(),
    }
}

/// `q (f a)` for each call `f a` in `v` whose result is refined by `q`.
fn refined_calls(ev: &Ev, prog: &Program, v: &Val, out: &mut Vec<Val>, depth: u32) {
    if depth > 8 {
        return;
    }
    match ev.force(v.clone()) {
        Val::Neutral(h, sp) => {
            if let Head::Global(g) = h
                && sp.iter().all(|e| matches!(e, Elim::App(..)))
            {
                for q in crate::walk::result_refinements(prog, ev, g, &sp) {
                    out.push(ev.apply(q, v.clone(), Icit::Expl));
                }
            }
            for e in &sp {
                if let Elim::App(a, _) = e {
                    refined_calls(ev, prog, a, out, depth + 1);
                }
            }
        }
        Val::Con(_, args) => {
            for a in &args {
                refined_calls(ev, prog, a, out, depth + 1);
            }
        }
        _ => {}
    }
}
