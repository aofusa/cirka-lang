//! Refinement types (spec-v0.0.4.md §3): every value checked against
//! `{x : A | p}` (marked `primRefine` by elaboration) must satisfy `p`.
//! An obligation is discharged by evaluation when `p` reduces to `True`,
//! and otherwise by Z3, under what the path to the value has learnt (the
//! matched constructors and literals, the conditions of `if`), the
//! refinements of the binders in scope and of the results of the
//! functions it calls.

use crate::measure::{Tr, smt_names};
use crate::walk::{Assume, Facts, Obligation};
use cirka_diagnostics::Diagnostic;
use cirka_smt::{Expr, Outcome, Query, Solver};
use cirkalang_elab::eval::Ev;
use cirkalang_elab::tt::*;

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

/// Checks the obligations of one definition.
pub fn check(
    ev: &Ev,
    solver: Option<&Solver>,
    prog: &Program,
    facts: &Facts,
    diags: &mut Vec<Diagnostic>,
) {
    let names = smt_names(facts);
    for ob in &facts.obligations {
        let Some(site) = prog.refine_sites.get(ob.site) else {
            continue;
        };
        if let Some(d) = obligation(ev, solver, prog, facts, &names, ob) {
            diags.push(d.with_label(site.span, format!("in `{}`", site.def)));
        }
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
