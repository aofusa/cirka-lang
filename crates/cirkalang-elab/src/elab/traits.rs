//! Traits (spec-v0.0.2.md §3): generation of selectors and default
//! methods, implementations as dictionaries, instance resolution and the
//! constraints that are solved after a definition has been elaborated.

use super::send::SendNo;
use super::*;
use std::collections::HashSet;

/// Pattern variables of instance heads are neutral variables at levels
/// from here on (context levels are always smaller).
const PATTERN_BASE: u32 = 1 << 28;

/// Depth limit of instance search (the goals of an `impl` may grow).
const MAX_DEPTH: u32 = 32;

pub(super) enum Goal {
    /// An instance-implicit argument of this type.
    Instance(Val),
    /// An integer literal whose type was unknown when it was checked.
    Int { n: u128, neg: bool, ty: Val },
}

/// A constraint: the meta `meta`, created in `cx`, is to be solved.
pub(super) struct Pending {
    pub meta: Term,
    pub cx: Cxt,
    pub goal: Goal,
    pub span: Span,
    /// Created in an erased value of a definition body that starts at
    /// this level (BUG-0033): the instance is used as evidence.
    pub erased: Option<u32>,
}

pub(super) enum Search {
    Found(Term),
    NotFound,
    /// The answer depends on a meta that is not solved yet.
    Stuck,
}

/// What selects the instances that can match a type (the first argument
/// of a goal or of an instance head).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum HeadKey {
    /// A data type or a primitive type (former), possibly applied.
    Con(GName),
    /// Not known yet.
    Meta,
    /// Anything else (a variable, a function type, ...).
    Other,
}

/// How two effects compare (spec-v0.0.7.md §2.3).
#[derive(Clone, Copy, PartialEq)]
enum Rel {
    Same,
    /// Different type constructors somewhere: never the same.
    Apart,
    /// Not known yet (metas), or not determined by the types (variables).
    Unknown,
}

impl Rel {
    fn and(self, o: Rel) -> Rel {
        match (self, o) {
            (Rel::Apart, _) | (_, Rel::Apart) => Rel::Apart,
            (Rel::Unknown, _) | (_, Rel::Unknown) => Rel::Unknown,
            _ => Rel::Same,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum M {
    Yes,
    No,
    Stuck,
}

impl M {
    fn and(self, o: M) -> M {
        match (self, o) {
            (M::No, _) | (_, M::No) => M::No,
            (M::Stuck, _) | (_, M::Stuck) => M::Stuck,
            _ => M::Yes,
        }
    }
}

impl<'r> Elab<'r> {
    /// The trait `ty` is an application of, if any.
    pub(super) fn trait_of(&self, ty: &Val) -> Option<GName> {
        match self.whnf(ty) {
            Val::Data(g, _) if self.prog.traits.contains_key(&g) => Some(g),
            _ => None,
        }
    }

    // ---- constraints ----

    /// Tries to solve the pending constraints; returns whether any was
    /// solved. With `finish`, undetermined literals are defaulted to `Int`
    /// and what remains is reported.
    pub(super) fn solve_pending(&self, finish: bool) -> bool {
        let mut any = false;
        loop {
            let items = std::mem::take(&mut *self.pending.borrow_mut());
            let mut rest = Vec::new();
            let mut progress = false;
            for p in items {
                if self.try_solve(&p) {
                    progress = true;
                } else {
                    rest.push(p);
                }
            }
            {
                // Solving may have added constraints.
                let mut pending = self.pending.borrow_mut();
                rest.append(&mut pending);
                *pending = rest;
            }
            any |= progress;
            // Effects are improved before literals are defaulted: the
            // state of `n <- get; put (n + 1)` comes from the row.
            if !progress && !(finish && (self.improve_effects() || self.default_literals())) {
                break;
            }
        }
        if finish {
            for p in std::mem::take(&mut *self.pending.borrow_mut()) {
                self.report_stuck(&p);
            }
            self.check_shareable();
        }
        any
    }

    /// Defaults the literals whose type is still unknown to `Int`.
    pub(super) fn default_literals(&self) -> bool {
        let Some(int) = self.prog.builtins.int else {
            return false;
        };
        let int = Val::Neutral(Head::Global(int), Vec::new());
        let mut any = false;
        for p in self.pending.borrow().iter() {
            if let Goal::Int { ty, .. } = &p.goal
                && matches!(self.whnf(ty), Val::Neutral(Head::Meta(_), _))
                && self.ev().unify(p.cx.lvl(), ty, &int).is_ok()
            {
                any = true;
            }
        }
        any
    }

    fn try_solve(&self, p: &Pending) -> bool {
        match &p.goal {
            Goal::Int { n, neg, ty } => {
                let wty = self.whnf(ty);
                if matches!(wty, Val::Neutral(Head::Meta(_), _)) {
                    return false;
                }
                let mut cx = p.cx.clone();
                let t = self.check_int(&mut cx, *n, *neg, &wty, p.span);
                self.assign(p, &t);
                true
            }
            Goal::Instance(goal) => match self.search(&p.cx, goal, 0) {
                Search::Found(t) => {
                    // An instance may take a value at run time (a variable
                    // declared `{n : Int}`); it cannot come from an erased
                    // variable.
                    if let Some(x) = self.erased_runtime_var(&p.cx, &t) {
                        self.report(
                            self.error(
                                "E0304",
                                p.span,
                                format!(
                                    "erased variable `{x}` is used at runtime by the instance for `{}`",
                                    self.show(&p.cx, goal)
                                ),
                            )
                            .with_note("the implementation takes this value at run time")
                            .with_help(format!(
                                "bind `{x}` with multiplicity ω (`{{ω {x} : ...}}` for an implicit argument)"
                            )),
                        );
                        self.assign(p, &Term::Error);
                        return true;
                    }
                    if let Some(entry) = p.erased {
                        self.erased_instance(p, goal, &t, entry);
                    }
                    self.assign(p, &t);
                    true
                }
                Search::Stuck => false,
                Search::NotFound => {
                    let send = self.prog.builtins.send_trait;
                    let has = self.prog.builtins.has_trait;
                    match self.whnf(goal) {
                        Val::Data(tr, args) if Some(tr) == send && args.len() == 1 => {
                            self.report(self.send_error(&p.cx, &args[0].0, p.span));
                        }
                        Val::Data(tr, args) if Some(tr) == has && args.len() == 2 => {
                            let e = self.show(&p.cx, &args[0].0);
                            let ep = if e.contains(' ') {
                                format!("({e})")
                            } else {
                                e.clone()
                            };
                            self.report(
                                self.error(
                                    "E1262",
                                    p.span,
                                    format!("the effect `{e}` is not handled"),
                                )
                                .with_note(format!(
                                    "the effects here are `{}`",
                                    self.show(&p.cx, &args[1].0)
                                ))
                                .with_help(format!(
                                    "install a handler of `{e}` around the computation \
                                         (`handle`, `handleWith`, ...), or add `Has {ep} es` to \
                                         the constraints of the signature"
                                )),
                            );
                        }
                        _ => self.report(
                            self.error(
                                "E1253",
                                p.span,
                                format!("no instance for `{}`", self.show(&p.cx, goal)),
                            )
                            .with_help("add an `impl`, or a constraint to the signature"),
                        ),
                    }
                    self.assign(p, &Term::Error);
                    true
                }
            },
        }
    }

    /// An instance `t` used in an erased value starting at level `entry`
    /// (BUG-0033): its methods are never called there, so they must be
    /// total. An implementation's dictionary is a global, whose totality
    /// `cirkalang-check` decides (with all its methods); a dictionary
    /// received at run time (a constraint of the definition) is only known
    /// to the callers, which E0507 reports.
    fn erased_instance(&self, p: &Pending, goal: &Val, t: &Term, entry: u32) {
        fn globals(t: &Term, out: &mut Vec<GName>) {
            match t {
                Term::Global(g) => out.push(*g),
                Term::App(f, a, _) => {
                    globals(f, out);
                    globals(a, out);
                }
                _ => {}
            }
        }
        let mut h = t;
        while let Term::App(f, _, _) = h {
            h = f;
        }
        if let Term::Var(i) = h
            && let Some(l) = p.cx.lvl().checked_sub(1 + i)
            && l < entry
            && p.cx.mults.get(l as usize).is_some_and(|m| *m != Mult::Zero)
        {
            self.report(
                self.error(
                    "E0507",
                    p.span,
                    format!(
                        "the instance for `{}` of the caller is used in an erased position",
                        self.show(&p.cx, goal)
                    ),
                )
                .with_note(
                    "an erased value is never computed, so the methods used in it are never \
                     called: the caller's implementation may panic or loop, and their results \
                     would prove anything",
                )
                .with_help("pass the proof at run time, or use an implementation known here"),
            );
            return;
        }
        let mut gs = Vec::new();
        globals(t, &mut gs);
        self.erased_uses
            .borrow_mut()
            .extend(gs.into_iter().map(|g| (g, p.span)));
    }

    /// An erased variable of `cx` that the dictionary `t` passes at run
    /// time (to a binder of multiplicity other than 0).
    fn erased_runtime_var(&self, cx: &Cxt, t: &Term) -> Option<Name> {
        let mut args = Vec::new();
        let mut h = t;
        while let Term::App(f, a, _) = h {
            args.push(&**a);
            h = f;
        }
        args.reverse();
        match h {
            Term::Var(i) if args.is_empty() => {
                let l = cx.lvl().checked_sub(1 + i)?;
                (cx.mults.get(l as usize) == Some(&Mult::Zero))
                    .then(|| cx.names[l as usize].clone())
            }
            Term::Global(g) => {
                let mut v = self.global_type(*g)?;
                for a in args {
                    let Val::Pi(_, m, _, _, c) = self.whnf(&v) else {
                        return None;
                    };
                    if m != Mult::Zero
                        && let Some(x) = self.erased_runtime_var(cx, a)
                    {
                        return Some(x);
                    }
                    v = self.ev().inst(&c, self.eval(cx, a));
                }
                None
            }
            _ => None,
        }
    }

    fn assign(&self, p: &Pending, t: &Term) {
        let (Term::Meta(m) | Term::InsertedMeta(m, _)) = &p.meta else {
            return;
        };
        if matches!(t, Term::Error) {
            self.metas.sol.borrow_mut()[*m as usize] = Some(Val::Error);
            return;
        }
        let mv = self.eval(&p.cx, &p.meta);
        let v = self.eval(&p.cx, t);
        if self.ev().unify(p.cx.lvl(), &mv, &v).is_err() {
            self.report(
                Diagnostic::ice(format!(
                    "the solution of a constraint escapes its scope: `{}`",
                    self.show(&p.cx, &v)
                ))
                .with_label(p.span, ""),
            );
        }
    }

    fn report_stuck(&self, p: &Pending) {
        if let (Goal::Instance(goal), Term::Meta(m) | Term::InsertedMeta(m, _)) = (&p.goal, &p.meta)
        {
            let Val::Data(_, gargs) = self.whnf(goal) else {
                return;
            };
            // Already reported through the goal that shares this type.
            if gargs
                .iter()
                .any(|(a, _)| matches!(self.whnf(a), Val::Error))
            {
                self.metas.sol.borrow_mut()[*m as usize] = Some(Val::Error);
                return;
            }
            let d = self.has_stuck(p, goal).unwrap_or_else(|| {
                self.error(
                    "E1254",
                    p.span,
                    format!(
                        "cannot determine which instance of `{}` to use",
                        self.show(&p.cx, goal)
                    ),
                )
                .with_help("add a type annotation")
            });
            self.report(d);
            for (a, _) in &gargs {
                if let Val::Neutral(Head::Meta(t), _) = self.whnf(a) {
                    self.metas.sol.borrow_mut()[t as usize] = Some(Val::Error);
                }
            }
            self.metas.sol.borrow_mut()[*m as usize] = Some(Val::Error);
        }
    }

    /// E1254 for a stuck `Has e es` whose row has an effect before `e` that
    /// may be `e` (a variable `f` in `f :: es`): which handler the operation
    /// uses depends on it (spec-v0.0.7.md §2.3).
    fn has_stuck(&self, p: &Pending, goal: &Val) -> Option<Diagnostic> {
        let Val::Data(tr, gargs) = self.whnf(goal) else {
            return None;
        };
        let ([(e, _), (row, _)], Some(has)) = (gargs.as_slice(), self.prog.builtins.has_trait)
        else {
            return None;
        };
        if tr != has {
            return None;
        }
        let lvl = p.cx.lvl();
        let mut r = self.whnf(row);
        for _ in 0..MAX_DEPTH {
            let Val::Con(c, args) = &r else { return None };
            if Some(*c) != self.prog.builtins.cons || args.len() < 2 {
                return None;
            }
            let f = &args[args.len() - 2];
            match self.effect_rel(lvl, e, f) {
                Rel::Apart => r = self.whnf(&args[args.len() - 1]),
                Rel::Same => return None,
                Rel::Unknown => {
                    let es = self.show(&p.cx, e);
                    let fs = self.show(&p.cx, f);
                    let paren = |s: &str| {
                        if s.contains(' ') {
                            format!("({s})")
                        } else {
                            s.to_string()
                        }
                    };
                    return Some(
                        self.error(
                            "E1254",
                            p.span,
                            format!("cannot tell whether the effect `{es}` is `{fs}`"),
                        )
                        .with_note(format!(
                            "`{fs}` comes first in the effects `{}`: if it is `{es}`, its \
                             handler is the one used",
                            self.show(&p.cx, row)
                        ))
                        .with_help(format!(
                            "add `Has {} {}` to the constraints of the signature",
                            paren(&es),
                            paren(&self.show(&p.cx, row))
                        )),
                    );
                }
            }
        }
        None
    }

    // ---- instance search ----

    pub(super) fn search(&self, cx: &Cxt, goal: &Val, depth: u32) -> Search {
        if depth > MAX_DEPTH {
            return Search::NotFound;
        }
        let Val::Data(tr, gargs) = self.whnf(goal) else {
            return if matches!(self.whnf(goal), Val::Neutral(Head::Meta(_), _)) {
                Search::Stuck
            } else {
                Search::NotFound
            };
        };
        let b = &self.prog.builtins;
        // `Send` follows from the structure of the type (spec-v0.0.5.md
        // §1.1): it has no implementations.
        if Some(tr) == b.send_trait {
            let Some((t, _)) = gargs.first() else {
                return Search::NotFound;
            };
            return match self.send_check(cx, t) {
                Ok(()) => Search::Found(self.send_dict(tr, &self.quote(cx, t))),
                Err(SendNo::Stuck) => Search::Stuck,
                Err(SendNo::No { .. }) => Search::NotFound,
            };
        }
        // `Drop` / `Release` of a value that holds only memory do nothing
        // (spec-v0.0.2.md §5.3), whatever the implementations are.
        let fin = if Some(tr) == b.drop_trait {
            Some((Fin::Drop, b.drop_free))
        } else if Some(tr) == b.release_trait {
            Some((Fin::Release, b.release_free))
        } else {
            None
        };
        if let (Some((_, Some(free))), Some((t, _))) = (fin, gargs.first())
            && !matches!(self.whnf(t), Val::Neutral(Head::Meta(_), _))
            && self.drop_status(cx, t) == DropStatus::Free
        {
            let tq = self.quote(cx, t);
            return Search::Found(self.fin_dict_term(
                tr,
                &tq,
                Term::App(Rc::new(Term::Global(free)), Rc::new(tq.clone()), Icit::Impl),
            ));
        }
        // Each argument is forced once, rather than by every instance.
        let gargs: Vec<(Val, Icit)> = gargs.into_iter().map(|(a, i)| (self.whnf(&a), i)).collect();
        let mut stuck = false;
        for l in (0..cx.lvl()).rev() {
            if cx.mults[l as usize] == Mult::Zero {
                continue;
            }
            let t = self.ty_val(cx, l);
            match self.local_candidate(cx, &Term::Var(cx.ix(l)), &t, tr, &gargs) {
                Search::Found(x) => return Search::Found(x),
                Search::Stuck => stuck = true,
                Search::NotFound => {}
            }
        }
        if Some(tr) == b.has_trait {
            if stuck {
                return Search::Stuck;
            }
            return self.has_search(cx, tr, &gargs, depth);
        }
        for i in self.candidates(tr, &gargs) {
            match self.try_impl(cx, &i, &gargs, depth) {
                Search::Found(x) => {
                    self.note_impl_use(i.dict);
                    return Search::Found(x);
                }
                Search::Stuck => stuck = true,
                Search::NotFound => {}
            }
        }
        // Nothing is known yet about a parameter that is still a meta.
        let open = gargs
            .iter()
            .any(|(a, _)| matches!(self.whnf(a), Val::Neutral(Head::Meta(_), _)));
        if stuck || open {
            return Search::Stuck;
        }
        // A value that can be dropped can be released.
        if let (Some((Fin::Release, _)), Some(drop_tr), Some(by_drop), Some((t, _))) =
            (fin, b.drop_trait, b.release_by_drop, gargs.first())
        {
            let dg = Val::Data(drop_tr, vec![(t.clone(), Icit::Expl)]);
            match self.search(cx, &dg, depth + 1) {
                Search::Found(d) => {
                    let tq = self.quote(cx, t);
                    let m = Term::App(
                        Rc::new(Term::App(
                            Rc::new(Term::Global(by_drop)),
                            Rc::new(tq.clone()),
                            Icit::Impl,
                        )),
                        Rc::new(d),
                        Icit::Impl,
                    );
                    return Search::Found(self.fin_dict_term(tr, &tq, m));
                }
                Search::Stuck => return Search::Stuck,
                Search::NotFound => {}
            }
        }
        Search::NotFound
    }

    /// `Has e es` (spec-v0.0.7.md §2.3): the handler at the head of the
    /// row when it is `e`, else `e` in the rest of the row. The two
    /// implementations of `Std.Effect` overlap: the compiler chooses
    /// between them from the shape of the row.
    fn has_search(&self, cx: &Cxt, tr: GName, gargs: &[(Val, Icit)], depth: u32) -> Search {
        let [(e, _), (row, _)] = gargs else {
            return Search::NotFound;
        };
        let impls: Vec<ImplInfo> = self
            .prog
            .impls
            .get(&tr)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|i| self.rp.info(i.dict).module == EFFECT_MODULE)
                    .collect()
            })
            .unwrap_or_default();
        let (Some(here), Some(there)) = (
            impls.iter().find(|i| i.nctx == 0),
            impls.iter().find(|i| i.nctx == 1),
        ) else {
            return Search::NotFound;
        };
        let i = match self.whnf(row) {
            Val::Con(c, args) if Some(c) == self.prog.builtins.cons && args.len() >= 2 => {
                match self.effect_rel(cx.lvl(), e, &args[args.len() - 2]) {
                    Rel::Same => here,
                    Rel::Apart => there,
                    Rel::Unknown => return Search::Stuck,
                }
            }
            Val::Neutral(Head::Meta(_), _) => return Search::Stuck,
            _ => return Search::NotFound,
        };
        let r = self.try_impl(cx, i, gargs, depth);
        if let Search::Found(_) = r {
            self.note_impl_use(i.dict);
        }
        r
    }

    /// Whether two effects are the same type, can never be the same, or
    /// may be (a variable may stand for either).
    fn effect_rel(&self, lvl: u32, a: &Val, b: &Val) -> Rel {
        let a = self.whnf(a);
        let b = self.whnf(b);
        let rigid = |v: &Val| match v {
            Val::Data(..) | Val::Con(..) | Val::Pi(..) | Val::Type | Val::LType | Val::Lit(_) => {
                true
            }
            Val::Neutral(Head::Global(g), _) => self.prog.prim_of(*g).is_some(),
            _ => false,
        };
        let all = |xs: Vec<(&Val, &Val)>| {
            xs.into_iter()
                .fold(Rel::Same, |r, (x, y)| r.and(self.effect_rel(lvl, x, y)))
        };
        match (&a, &b) {
            (Val::Data(d1, a1), Val::Data(d2, a2)) => {
                if d1 != d2 || a1.len() != a2.len() {
                    return Rel::Apart;
                }
                all(a1.iter().zip(a2).map(|(x, y)| (&x.0, &y.0)).collect())
            }
            (Val::Con(c1, a1), Val::Con(c2, a2)) => {
                if c1 != c2 || a1.len() != a2.len() {
                    return Rel::Apart;
                }
                all(a1.iter().zip(a2).collect())
            }
            // The same variable, or the same unknown (equal whatever it
            // becomes).
            (Val::Neutral(h1, s1), Val::Neutral(h2, s2)) if h1 == h2 && s1.len() == s2.len() => {
                s1.iter().zip(s2).fold(Rel::Same, |r, e| {
                    r.and(match e {
                        (Elim::App(x, _), Elim::App(y, _)) => self.effect_rel(lvl, x, y),
                        _ => Rel::Unknown,
                    })
                })
            }
            (Val::Pi(_, m1, i1, a1, c1), Val::Pi(_, m2, i2, a2, c2)) => {
                if m1 != m2 || i1 != i2 {
                    return Rel::Apart;
                }
                let x = Val::var(lvl);
                self.effect_rel(lvl, a1, a2).and(self.effect_rel(
                    lvl + 1,
                    &self.ev().inst(c1, x.clone()),
                    &self.ev().inst(c2, x),
                ))
            }
            (Val::Type, Val::Type) | (Val::LType, Val::LType) => Rel::Same,
            (Val::Lit(x), Val::Lit(y)) => {
                if x == y {
                    Rel::Same
                } else {
                    Rel::Apart
                }
            }
            _ if rigid(&a) && rigid(&b) => Rel::Apart,
            _ => Rel::Unknown,
        }
    }

    /// Improves a stuck `Has e es` whose effect is not fully known (`get`
    /// in `State ?s`): `e` is the innermost effect of the row with the same
    /// type constructor (spec-v0.0.7.md §2.3). Returns whether it solved
    /// a meta (the constraints are then solved again before the next).
    fn improve_effects(&self) -> bool {
        let Some(has) = self.prog.builtins.has_trait else {
            return false;
        };
        let goals: Vec<(Cxt, Val)> = self
            .pending
            .borrow()
            .iter()
            .filter_map(|p| match &p.goal {
                Goal::Instance(g) => Some((p.cx.clone(), g.clone())),
                Goal::Int { .. } => None,
            })
            .collect();
        for (cx, goal) in goals {
            let Val::Data(tr, args) = self.whnf(&goal) else {
                continue;
            };
            if tr != has || args.len() != 2 {
                continue;
            }
            let e = self.whnf(&args[0].0);
            let Val::Data(d, _) = &e else { continue };
            if let Some(f) = self.innermost_effect(&cx, has, *d, &e, &args[1].0)
                && self.effect_rel(cx.lvl(), &e, &f) == Rel::Unknown
                && self.ev().unify(cx.lvl(), &e, &f).is_ok()
                && self.effect_rel(cx.lvl(), &e, &f) == Rel::Same
            {
                return true;
            }
        }
        false
    }

    /// The innermost effect of `row` with the type constructor `d` that is
    /// not apart from `e`: an element of the row, or the effect of a local
    /// `Has` constraint on the rest of the row.
    fn innermost_effect(&self, cx: &Cxt, has: GName, d: GName, e: &Val, row: &Val) -> Option<Val> {
        let candidate = |f: &Val| {
            matches!(f, Val::Data(d2, _) if *d2 == d)
                && self.effect_rel(cx.lvl(), e, f) != Rel::Apart
        };
        let mut row = self.whnf(row);
        for _ in 0..MAX_DEPTH {
            let Val::Con(c, args) = &row else { break };
            if Some(*c) != self.prog.builtins.cons || args.len() < 2 {
                break;
            }
            let f = self.whnf(&args[args.len() - 2]);
            if candidate(&f) {
                return Some(f);
            }
            if matches!(f, Val::Neutral(Head::Meta(_), _)) {
                return None;
            }
            row = self.whnf(&args[args.len() - 1]);
        }
        for l in (0..cx.lvl()).rev() {
            if cx.mults[l as usize] == Mult::Zero {
                continue;
            }
            let t = self.ty_val(cx, l);
            if let Val::Data(lt, largs) = self.whnf(&t)
                && lt == has
                && largs.len() == 2
                && self.effect_rel(cx.lvl(), &largs[1].0, &row) == Rel::Same
            {
                let f = self.whnf(&largs[0].0);
                if candidate(&f) {
                    return Some(f);
                }
            }
        }
        None
    }

    /// The instances of `tr` that can match the goal `gargs`, in the order
    /// they were declared: those whose key is the goal's and those without
    /// a key. An instance with another key cannot match; while the goal is
    /// a meta, it could only be stuck (the search is then stuck anyway).
    fn candidates(&self, tr: GName, gargs: &[(Val, Icit)]) -> Vec<ImplInfo> {
        let Some(all) = self.prog.impls.get(&tr) else {
            return Vec::new();
        };
        let key = match gargs.first() {
            Some((a, _)) => self.head_key(a),
            None => HeadKey::Other,
        };
        let pos = |k: Option<GName>| {
            self.prog
                .impl_index
                .get(&(tr, k))
                .map(Vec::as_slice)
                .unwrap_or(&[])
        };
        let mut ix: Vec<usize> = match key {
            HeadKey::Con(g) => pos(Some(g)).iter().chain(pos(None)).copied().collect(),
            HeadKey::Meta => pos(None).to_vec(),
            HeadKey::Other => return all.clone(),
        };
        ix.sort_unstable();
        ix.into_iter().map(|k| all[k]).collect()
    }

    /// The [`HeadKey`] of a type.
    pub(super) fn head_key(&self, v: &Val) -> HeadKey {
        match self.whnf(v) {
            Val::Data(d, _) => HeadKey::Con(d),
            // Primitive types do not reduce.
            Val::Neutral(Head::Global(g), _)
                if self.prog.prim_of(g).is_some_and(|p| p.is_type()) =>
            {
                HeadKey::Con(g)
            }
            Val::Neutral(Head::Meta(_), _) => HeadKey::Meta,
            _ => HeadKey::Other,
        }
    }

    /// The `key` of an instance.
    fn impl_key(&self, i: &ImplInfo) -> Option<GName> {
        match self
            .impl_head_args(i)
            .first()
            .map(|(a, _)| self.head_key(a))
        {
            Some(HeadKey::Con(g)) => Some(g),
            _ => None,
        }
    }

    /// The dictionary of `Drop t` / `Release t` with this method.
    fn fin_dict_term(&self, tr: GName, t: &Term, method: Term) -> Term {
        let con = self.prog.traits.get(&tr).map(|i| i.con).unwrap_or(tr);
        Term::App(
            Rc::new(Term::App(
                Rc::new(Term::Global(con)),
                Rc::new(t.clone()),
                Icit::Impl,
            )),
            Rc::new(method),
            Icit::Expl,
        )
    }

    /// A dictionary in scope, or one of its supertraits.
    pub(super) fn local_candidate(
        &self,
        cx: &Cxt,
        term: &Term,
        ty: &Val,
        tr: GName,
        gargs: &[(Val, Icit)],
    ) -> Search {
        let Val::Data(lt, largs) = self.whnf(ty) else {
            return Search::NotFound;
        };
        let Some(info) = self.prog.traits.get(&lt) else {
            return Search::NotFound;
        };
        let mut stuck = false;
        if lt == tr {
            let mut sub = Vec::new();
            match self.match_args(cx.lvl(), &largs, gargs, &mut sub) {
                M::Yes => return Search::Found(term.clone()),
                M::Stuck => stuck = true,
                M::No => {}
            }
        }
        for sel in &info.supers {
            let (st, sty) = self.project(cx, *sel, &largs, term);
            match self.local_candidate(cx, &st, &sty, tr, gargs) {
                Search::Found(x) => return Search::Found(x),
                Search::Stuck => stuck = true,
                Search::NotFound => {}
            }
        }
        if stuck {
            Search::Stuck
        } else {
            Search::NotFound
        }
    }

    /// `sel {args} dict` and its type.
    fn project(&self, cx: &Cxt, sel: GName, args: &[(Val, Icit)], dict: &Term) -> (Term, Val) {
        let mut t = Term::Global(sel);
        let mut v = self.global_type(sel).unwrap_or(Val::Error);
        for (a, _) in args {
            if let Val::Pi(_, _, _, _, c) = self.whnf(&v) {
                v = self.ev().inst(&c, a.clone());
            }
            t = Term::App(Rc::new(t), Rc::new(self.quote(cx, a)), Icit::Impl);
        }
        if let Val::Pi(_, _, _, _, c) = self.whnf(&v) {
            v = self.ev().inst(&c, self.eval(cx, dict));
        }
        (Term::App(Rc::new(t), Rc::new(dict.clone()), Icit::Impl), v)
    }

    fn try_impl(&self, cx: &Cxt, i: &ImplInfo, gargs: &[(Val, Icit)], depth: u32) -> Search {
        let Some(ty) = self.global_type(i.dict) else {
            return Search::NotFound;
        };
        let mut v = ty.clone();
        for k in 0..i.nvars {
            let Val::Pi(_, _, _, _, c) = self.whnf(&v) else {
                return Search::NotFound;
            };
            v = self.ev().inst(&c, Val::var(PATTERN_BASE + k as u32));
        }
        for _ in 0..i.nctx {
            let Val::Pi(_, _, _, _, c) = self.whnf(&v) else {
                return Search::NotFound;
            };
            v = self.ev().inst(&c, Val::Error);
        }
        let Val::Data(_, hargs) = self.whnf(&v) else {
            return Search::NotFound;
        };
        let mut sub: Vec<Option<Val>> = vec![None; i.nvars];
        match self.match_args(cx.lvl(), &hargs, gargs, &mut sub) {
            M::No => return Search::NotFound,
            M::Stuck => return Search::Stuck,
            M::Yes => {}
        }
        let mut t = Term::Global(i.dict);
        let mut v = ty;
        for s in sub {
            let a = s.unwrap_or(Val::Error);
            let Val::Pi(_, _, _, dom, c) = self.whnf(&v) else {
                return Search::NotFound;
            };
            // A variable of a shareable sort does not match a linear type
            // (spec-v0.0.2.md §5.1).
            if self.type_valued(cx.lvl(), &dom) && self.linear_arg(cx, &a, &dom) {
                return Search::NotFound;
            }
            t = Term::App(Rc::new(t), Rc::new(self.quote(cx, &a)), Icit::Impl);
            v = self.ev().inst(&c, a);
        }
        for _ in 0..i.nctx {
            let Val::Pi(_, _, _, dom, c) = self.whnf(&v) else {
                return Search::NotFound;
            };
            match self.search(cx, &dom, depth + 1) {
                Search::Found(d) => {
                    let dv = self.eval(cx, &d);
                    t = Term::App(Rc::new(t), Rc::new(d), Icit::Impl);
                    v = self.ev().inst(&c, dv);
                }
                other => return other,
            }
        }
        Search::Found(t)
    }

    fn match_args(
        &self,
        lvl: u32,
        pats: &[(Val, Icit)],
        goals: &[(Val, Icit)],
        sub: &mut Vec<Option<Val>>,
    ) -> M {
        if pats.len() != goals.len() {
            return M::No;
        }
        let mut r = M::Yes;
        for ((p, _), (g, _)) in pats.iter().zip(goals) {
            r = r.and(self.match_val(lvl, p, g, sub));
            if r == M::No {
                break;
            }
        }
        r
    }

    /// One-way matching of `pat` against `goal` (which is never assigned).
    /// Pattern variables are bound in `sub`.
    fn match_val(&self, lvl: u32, pat: &Val, goal: &Val, sub: &mut Vec<Option<Val>>) -> M {
        let pat = self.whnf(pat);
        let goal = self.whnf(goal);
        if let Val::Neutral(Head::Var(l), sp) = &pat
            && *l >= PATTERN_BASE
            && sp.is_empty()
        {
            let k = (*l - PATTERN_BASE) as usize;
            return match sub[k].clone() {
                None => {
                    sub[k] = Some(goal);
                    M::Yes
                }
                Some(prev) => self.match_val(lvl, &prev, &goal, sub),
            };
        }
        // The same unknown on both sides (a variable of the instance bound
        // twice, `Has e (e :: es)`): equal whatever it becomes.
        if let (Val::Neutral(Head::Meta(a), sa), Val::Neutral(Head::Meta(b), sb)) = (&pat, &goal)
            && a == b
            && sa.len() == sb.len()
        {
            let mut r = M::Yes;
            for e in sa.iter().zip(sb) {
                r = r.and(match e {
                    (Elim::App(x, _), Elim::App(y, _)) => self.match_val(lvl, x, y, sub),
                    _ => M::Stuck,
                });
            }
            return r;
        }
        if matches!(goal, Val::Neutral(Head::Meta(_), _))
            || matches!(pat, Val::Neutral(Head::Meta(_), _))
        {
            return M::Stuck;
        }
        match (&pat, &goal) {
            (Val::Data(d1, a1), Val::Data(d2, a2)) if d1 == d2 => self.match_args(lvl, a1, a2, sub),
            (Val::Con(c1, a1), Val::Con(c2, a2)) if c1 == c2 && a1.len() == a2.len() => {
                let mut r = M::Yes;
                for (x, y) in a1.iter().zip(a2) {
                    r = r.and(self.match_val(lvl, x, y, sub));
                }
                r
            }
            (Val::Neutral(h1, s1), Val::Neutral(h2, s2)) if h1 == h2 && s1.len() == s2.len() => {
                let mut r = M::Yes;
                for (x, y) in s1.iter().zip(s2) {
                    r = r.and(match (x, y) {
                        (Elim::App(a, _), Elim::App(b, _)) => self.match_val(lvl, a, b, sub),
                        _ => M::Stuck,
                    });
                }
                r
            }
            (Val::Pi(_, m1, i1, a1, c1), Val::Pi(_, m2, i2, a2, c2)) if m1 == m2 && i1 == i2 => {
                let r = self.match_val(lvl, a1, a2, sub);
                let x = Val::var(lvl);
                r.and(self.match_val(
                    lvl + 1,
                    &self.ev().inst(c1, x.clone()),
                    &self.ev().inst(c2, x),
                    sub,
                ))
            }
            (Val::Type, Val::Type) | (Val::LType, Val::LType) => M::Yes,
            (Val::Lit(x), Val::Lit(y)) if x == y => M::Yes,
            _ => M::No,
        }
    }

    // ---- traits ----

    /// The field types of the dictionary constructor for these parameters.
    fn dict_fields(&self, con: GName, args: &[(Val, Icit)]) -> Vec<Val> {
        let Some(mut v) = self.global_type(con) else {
            return Vec::new();
        };
        for (a, _) in args {
            if let Val::Pi(_, _, _, _, c) = self.whnf(&v) {
                v = self.ev().inst(&c, a.clone());
            }
        }
        let mut out = Vec::new();
        // Fields are not dependent (the resolver generates them unnamed).
        while let Val::Pi(_, _, _, a, c) = self.whnf(&v) {
            out.push((*a).clone());
            v = self.ev().inst(&c, Val::Error);
        }
        out
    }

    /// Selectors and the signatures of default methods.
    pub(super) fn trait_item(&mut self, t: &rast::Trait) {
        self.begin(self.prog.name(t.name).to_string());
        let Global::Con {
            ty: cty, nparams, ..
        } = self.prog.get(t.dict_con).clone()
        else {
            return;
        };
        let mut v = self.ev().eval(&Vec::new(), &cty);
        let mut params: Vec<(Name, Term)> = Vec::new();
        for k in 0..nparams {
            let Val::Pi(x, _, _, a, c) = self.whnf(&v) else {
                return;
            };
            params.push((x, self.ev().quote(k as u32, &a)));
            v = self.ev().inst(&c, Val::var(k as u32));
        }
        let lvl = nparams as u32;
        let mut ftys = Vec::new();
        while let Val::Pi(_, _, _, a, c) = self.whnf(&v) {
            ftys.push((*a).clone());
            v = self.ev().inst(&c, Val::Error);
        }
        let dict_ty = (0..lvl).fold(Term::Global(t.name), |f, k| {
            Term::App(Rc::new(f), Rc::new(Term::Var(lvl - 1 - k)), Icit::Expl)
        });
        let nsup = t.supers.len();
        let mut super_traits = Vec::new();
        let sels: Vec<GName> = t
            .supers
            .iter()
            .copied()
            .chain(t.methods.iter().map(|m| m.name))
            .collect();
        // A method that is not a function is shared by all the uses of an
        // implementation: its type must not be linear.
        for (j, m) in t.methods.iter().enumerate() {
            let Some(f) = ftys.get(nsup + j) else { break };
            let ty = params
                .iter()
                .rev()
                .fold(self.ev().quote(lvl, f), |b, (x, k)| {
                    Term::Pi(
                        x.clone(),
                        Mult::Zero,
                        Icit::Impl,
                        Rc::new(k.clone()),
                        Rc::new(b),
                    )
                });
            let short = self.prog.short_name(m.name).to_string();
            self.check_shared_type(
                &ty,
                m.span,
                &format!("method `{short}`"),
                &short,
                "the value of a method is shared by all the uses of an implementation",
            );
        }
        for (j, sel) in sels.iter().enumerate() {
            if j < nsup {
                match self.trait_of(&ftys[j]) {
                    Some(g) => super_traits.push(g),
                    None => self.report(self.error(
                        "E1250",
                        t.span,
                        format!(
                            "a supertrait of `{}` is not a trait",
                            self.prog.short_name(t.name)
                        ),
                    )),
                }
            }
            let fields: Vec<(Name, Mult, Term)> = ftys
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    (
                        Name::from(format!("f{i}")),
                        Mult::Many,
                        self.ev().quote(lvl + 1 + i as u32, f),
                    )
                })
                .collect();
            let ret = self.ev().quote(lvl + 1, &ftys[j]);
            let body = Term::Case(Rc::new(Case {
                scrut: Term::Var(0),
                ret: ret.clone(),
                alts: vec![Alt::Con {
                    con: t.dict_con,
                    fields,
                    refine: Vec::new(),
                    body: Term::Var((ftys.len() - 1 - j) as u32),
                }],
                default: None,
            }));
            let mut body = Term::Lam(
                "d$".into(),
                Mult::Many,
                Icit::Impl,
                Rc::new(dict_ty.clone()),
                Rc::new(body),
            );
            let mut ty = Term::Pi(
                "d$".into(),
                Mult::Many,
                Icit::Impl,
                Rc::new(dict_ty.clone()),
                Rc::new(ret),
            );
            for (x, k) in params.iter().rev() {
                body = Term::Lam(
                    x.clone(),
                    Mult::Zero,
                    Icit::Impl,
                    Rc::new(k.clone()),
                    Rc::new(body),
                );
                ty = Term::Pi(
                    x.clone(),
                    Mult::Zero,
                    Icit::Impl,
                    Rc::new(k.clone()),
                    Rc::new(ty),
                );
            }
            self.prog.globals[sel.0 as usize] = Global::Def {
                ty: ty.clone(),
                body: Some(body),
            };
            self.prog.order.push(*sel);
            if j >= nsup
                && let Some((dm, _)) = &t.methods[j - nsup].default
            {
                self.prog.globals[dm.0 as usize] = Global::Def { ty, body: None };
            }
        }
        self.prog.traits.insert(
            t.name,
            TraitInfo {
                con: t.dict_con,
                supers: t.supers.clone(),
                super_traits,
                defaults: t
                    .methods
                    .iter()
                    .map(|m| m.default.as_ref().map(|d| d.0))
                    .collect(),
            },
        );
    }

    /// E1257: a trait cannot be its own (indirect) supertrait.
    pub(super) fn check_supertrait_cycles(&self, traits: &[&rast::Trait]) {
        fn visit(p: &Program, g: GName, seen: &mut Vec<GName>, target: GName) -> bool {
            for s in p.traits.get(&g).map_or(&[][..], |t| &t.super_traits) {
                if *s == target
                    || (!seen.contains(s) && {
                        seen.push(*s);
                        visit(p, *s, seen, target)
                    })
                {
                    return true;
                }
            }
            false
        }
        for t in traits {
            if visit(&self.prog, t.name, &mut Vec::new(), t.name) {
                self.report(self.error(
                    "E1257",
                    t.span,
                    format!(
                        "trait `{}` is its own supertrait",
                        self.prog.short_name(t.name)
                    ),
                ));
            }
        }
    }

    pub(super) fn default_bodies(&mut self, t: &rast::Trait) {
        for m in &t.methods {
            let Some((dm, clauses)) = &m.default else {
                continue;
            };
            self.begin(self.prog.name(*dm).to_string());
            let Global::Def { ty, .. } = self.prog.get(*dm).clone() else {
                continue;
            };
            let before = self.error_count();
            let mut cx = Cxt::default();
            let mut binders = Vec::new();
            let mut v = self.ev().eval(&Vec::new(), &ty);
            for k in 0..=t.params.len() {
                let Val::Pi(x, mm, i, a, c) = self.whnf(&v) else {
                    return;
                };
                let src = t.params.get(k).map(|s| s.as_str());
                let l = self.push(&mut cx, &x, mm, &a, None, t.span, src);
                binders.push((x, mm, i, self.ev().quote(l, &a)));
                v = self.ev().inst(&c, Val::var(l));
            }
            let short = self.prog.short_name(m.name).to_string();
            let body = self.elab_clauses(&mut cx, &short, clauses, &v, m.span);
            let body = self.finish(&cx, &body);
            // It takes the dictionary: its body is evaluated at each use.
            cx.holders.clear();
            let body = wrap_lams(binders, body);
            let ok = self.error_count() == before;
            let body = if ok {
                self.insert_finalizers(&ty, body, m.span)
            } else {
                body
            };
            self.prog.globals[dm.0 as usize] = Global::Def {
                ty,
                body: ok.then_some(body),
            };
            self.prog.order.push(*dm);
        }
    }

    // ---- implementations ----

    pub(super) fn impl_sig(&mut self, i: &rast::Impl) {
        self.begin(self.prog.name(i.name).to_string());
        // The variables of an `impl` of a trait over `LType` (`Drop`,
        // `Release`) range over `LType`.
        let over_ltype = self.global_type(i.trait_).is_some_and(
            |t| matches!(self.whnf(&t), Val::Pi(_, _, _, d, _) if matches!(self.whnf(&d), Val::LType)),
        );
        self.ltype_vars.set(over_ltype);
        let mut cx = Cxt::default();
        let mut binders: Vec<(Name, Mult, Term)> = Vec::new();
        for (j, (n, sp)) in i.vars.iter().enumerate() {
            let k = match i.kinds.get(j) {
                Some(k) => self.check_type(&mut cx, k),
                None => self.fresh_meta(&cx, *sp),
            };
            let kv = self.eval(&cx, &k);
            // A variable declared with the type of a value (`{n : Int}`)
            // is passed at run time: the methods may use it
            // (`impl {n : Int} -> Storable a => Storable (CArray n a)`,
            // spec-v0.0.6.md §1.2). Types are erased.
            let m = if i.kinds.get(j).is_some() && !self.is_sort(cx.lvl(), &kv) {
                Mult::Many
            } else {
                Mult::Zero
            };
            binders.push((n.as_str().into(), m, k));
            self.push(&mut cx, n, m, &kv, None, *sp, Some(n));
        }
        for c in &i.context {
            let t = self.check_type(&mut cx, c);
            let tv = self.eval(&cx, &t);
            if self.trait_of(&tv).is_none() && !matches!(self.whnf(&tv), Val::Error) {
                self.report(self.error("E1250", c.span, "a constraint must be a trait"));
            }
            binders.push(("_".into(), Mult::Many, t));
            self.push(&mut cx, "_", Mult::Many, &tv, None, c.span, None);
        }
        let head = self.check_type(&mut cx, &i.head);
        self.ltype_vars.set(false);
        // A finalizer runs once per value, so only linear types can have
        // one (shared values are freed by reference counting).
        let b = &self.prog.builtins;
        if (Some(i.trait_) == b.drop_trait || Some(i.trait_) == b.release_trait)
            && let Val::Data(_, args) = self.whnf(&self.eval(&cx, &head))
            && let Some((t, _)) = args.first()
            && self.univ_of(&cx, t) == Univ::Type
        {
            self.report(
                self.error(
                    "E1258",
                    i.head.span,
                    format!(
                        "`{}` can only be implemented for linear types, but `{}` is shareable",
                        self.prog.short_name(i.trait_),
                        self.show(&cx, t)
                    ),
                )
                .with_note("shared values are freed by reference counting and have no finalizer")
                .with_help("declare the data type in `LType`"),
            );
        }
        if Some(i.trait_) == b.has_trait && self.rp.info(i.name).module != EFFECT_MODULE {
            self.report(
                self.error("E1259", i.head.span, "`Has` cannot be implemented")
                    .with_note("the compiler finds an effect in a row from the shape of the row")
                    .with_help(
                        "install a handler with `handle`, or add `Has E es =>` to the signature",
                    ),
            );
        }
        if Some(i.trait_) == b.send_trait {
            self.report(
                self.error("E1259", i.head.span, "`Send` cannot be implemented")
                    .with_note(
                        "whether values of a type can be sent to another thread follows \
                         from the types of its fields",
                    )
                    .with_help(
                        "to keep a type on its thread, give it a field of type `ThreadBound`",
                    ),
            );
        }
        let mut ty = head;
        for (x, m, a) in binders.into_iter().rev() {
            ty = Term::Pi(x, m, Icit::Impl, Rc::new(a), Rc::new(ty));
        }
        self.solve_pending(true);
        let ty = self.zonk(0, &ty);
        let (nvars, nctx) = (i.vars.len(), i.context.len());
        // Every variable must be determined by the head.
        let mut h = &ty;
        for _ in 0..nvars + nctx {
            if let Term::Pi(_, _, _, _, b) = h {
                h = b;
            }
        }
        for (k, (n, sp)) in i.vars.iter().enumerate() {
            if !term_mentions(h, (nvars + nctx - 1 - k) as u32) {
                self.report(
                    self.error(
                        "E1256",
                        *sp,
                        format!("`{n}` does not occur in the head of the `impl`"),
                    )
                    .with_note("instances are selected by matching their head"),
                );
            }
        }
        self.prog.globals[i.name.0 as usize] = Global::Def { ty, body: None };
        let mut info = ImplInfo {
            dict: i.name,
            nvars,
            nctx,
            key: None,
        };
        info.key = self.impl_key(&info);
        let list = self.prog.impls.entry(i.trait_).or_default();
        list.push(info);
        let pos = list.len() - 1;
        self.prog
            .impl_index
            .entry((i.trait_, info.key))
            .or_default()
            .push(pos);
    }

    /// The arguments of the head of an instance, with pattern variables.
    pub(super) fn impl_head_args(&self, i: &ImplInfo) -> Vec<(Val, Icit)> {
        let Some(mut v) = self.global_type(i.dict) else {
            return Vec::new();
        };
        for k in 0..i.nvars + i.nctx {
            let Val::Pi(_, _, _, _, c) = self.whnf(&v) else {
                return Vec::new();
            };
            v = self.ev().inst(&c, Val::var(PATTERN_BASE + k as u32));
        }
        match self.whnf(&v) {
            Val::Data(_, a) => a,
            _ => Vec::new(),
        }
    }

    fn overlaps(&self, a: &Val, b: &Val) -> bool {
        let a = self.whnf(a);
        let b = self.whnf(b);
        let is_var = |v: &Val| matches!(v, Val::Neutral(Head::Var(l), _) if *l >= PATTERN_BASE);
        if is_var(&a) || is_var(&b) {
            return true;
        }
        match (&a, &b) {
            (Val::Data(d1, a1), Val::Data(d2, a2)) => {
                d1 == d2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|(x, y)| self.overlaps(&x.0, &y.0))
            }
            (Val::Con(c1, a1), Val::Con(c2, a2)) => {
                c1 == c2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|(x, y)| self.overlaps(x, y))
            }
            (Val::Neutral(h1, s1), Val::Neutral(h2, s2)) => {
                h1 == h2
                    && s1.len() == s2.len()
                    && s1.iter().zip(s2).all(|(x, y)| match (x, y) {
                        (Elim::App(p, _), Elim::App(q, _)) => self.overlaps(p, q),
                        _ => true,
                    })
            }
            (Val::Pi(_, _, _, x, _), Val::Pi(_, _, _, y, _)) => self.overlaps(x, y),
            (Val::Type, Val::Type) | (Val::LType, Val::LType) => true,
            (Val::Lit(x), Val::Lit(y)) => x == y,
            _ => false,
        }
    }

    /// Orphan rule (E1251) and overlapping instances (E1252).
    pub(super) fn check_coherence(&self, impls: &[&rast::Impl]) {
        let module_of = |g: GName| &self.rp.info(g).module;
        let mut orphans = HashSet::new();
        // The heads of the instances and their keys, computed once: only
        // instances with the same key (or without one) can overlap.
        let heads: Vec<Vec<(Val, Icit)>> = impls
            .iter()
            .map(|i| {
                self.impl_head_args(&ImplInfo {
                    dict: i.name,
                    nvars: i.vars.len(),
                    nctx: i.context.len(),
                    key: None,
                })
            })
            .collect();
        let keys: Vec<HeadKey> = heads
            .iter()
            .map(|h| match h.first() {
                Some((a, _)) => self.head_key(a),
                None => HeadKey::Other,
            })
            .collect();
        for (i, head) in impls.iter().zip(&heads) {
            let here = module_of(i.name);
            let local_head = head.iter().any(|(a, _)| match self.whnf(a) {
                Val::Data(d, _) => module_of(d) == here,
                Val::Neutral(Head::Global(g), _) => module_of(g) == here,
                _ => false,
            });
            if module_of(i.trait_) != here && !local_head {
                self.report(
                    self.error(
                        "E1251",
                        i.span,
                        format!("orphan `impl` of `{}`", self.prog.short_name(i.trait_)),
                    )
                    .with_note(
                        "an `impl` must be in the module of the trait or of a type in its head",
                    ),
                );
                orphans.insert(i.name);
            }
        }
        for (a, ia) in impls.iter().enumerate() {
            for (b, ib) in impls.iter().enumerate().skip(a + 1) {
                // The two implementations of `Has` overlap; the compiler
                // chooses between them (spec-v0.0.7.md §2.3).
                if ib.trait_ != ia.trait_
                    || Some(ia.trait_) == self.prog.builtins.has_trait
                    || orphans.contains(&ia.name)
                    || orphans.contains(&ib.name)
                    || matches!((keys[a], keys[b]), (HeadKey::Con(x), HeadKey::Con(y)) if x != y)
                {
                    continue;
                }
                let (x, y) = (&heads[a], &heads[b]);
                if x.len() == y.len() && x.iter().zip(y).all(|(p, q)| self.overlaps(&p.0, &q.0)) {
                    self.report(
                        self.error(
                            "E1252",
                            ib.span,
                            format!(
                                "overlapping `impl`s of `{}`",
                                self.prog.short_name(ib.trait_)
                            ),
                        )
                        .with_secondary(ia.span, "the other one"),
                    );
                }
            }
        }
    }

    fn eta_expand(&self, lvl: u32, f: Val, ty: &Val) -> Term {
        match self.whnf(ty) {
            Val::Pi(x, m, i, a, c) => {
                let cod = self.ev().inst(&c, Val::var(lvl));
                let fx = self.ev().apply(f, Val::var(lvl), i);
                let b = self.eta_expand(lvl + 1, fx, &cod);
                Term::Lam(x, m, i, Rc::new(self.ev().quote(lvl, &a)), Rc::new(b))
            }
            _ => self.ev().quote(lvl, &f),
        }
    }

    /// Method definitions and the dictionary of an instance.
    pub(super) fn impl_body(&mut self, i: &rast::Impl) {
        let name = self.prog.name(i.name).to_string();
        self.begin(name.clone());
        let Global::Def { ty: dty, .. } = self.prog.get(i.name).clone() else {
            return;
        };
        let Some(info) = self.prog.traits.get(&i.trait_).cloned() else {
            return;
        };
        let before = self.error_count();
        let mut cx = Cxt::default();
        let mut binders = Vec::new();
        let mut v = self.ev().eval(&Vec::new(), &dty);
        let nbind = i.vars.len() + i.context.len();
        for k in 0..nbind {
            let Val::Pi(x, m, ic, a, c) = self.whnf(&v) else {
                return;
            };
            let src = i.vars.get(k).map(|s| s.0.as_str());
            let l = self.push(&mut cx, &x, m, &a, None, i.span, src);
            binders.push((x, m, ic, self.ev().quote(l, &a)));
            v = self.ev().inst(&c, Val::var(l));
        }
        let Val::Data(_, args) = self.whnf(&v) else {
            return;
        };
        let ftys = self.dict_fields(info.con, &args);
        let nsup = info.supers.len();
        if ftys.len() != nsup + i.methods.len() {
            return;
        }
        // `dict {vars} ctx` (the arguments are this context's variables).
        let applied = |g: GName| {
            (0..nbind).fold(Term::Global(g), |f, k| {
                Term::App(
                    Rc::new(f),
                    Rc::new(Term::Var((nbind - 1 - k) as u32)),
                    Icit::Impl,
                )
            })
        };
        let wrap_pis = |t: Term| {
            binders.iter().rev().fold(t, |b, (x, m, ic, a)| {
                Term::Pi(x.clone(), *m, *ic, Rc::new(a.clone()), Rc::new(b))
            })
        };
        // The methods.
        for (j, m) in i.methods.iter().enumerate() {
            let fty = &ftys[nsup + j];
            self.begin(self.prog.name(m.def).to_string());
            let ty = wrap_pis(self.quote(&cx, fty));
            let m_before = self.error_count();
            let short = self.prog.short_name(m.method).to_string();
            let body = if m.clauses.is_empty() {
                match info.defaults[j] {
                    Some(dm) => {
                        let mut t = Term::Global(dm);
                        for (a, _) in &args {
                            t = Term::App(Rc::new(t), Rc::new(self.quote(&cx, a)), Icit::Impl);
                        }
                        let t = Term::App(Rc::new(t), Rc::new(applied(i.name)), Icit::Impl);
                        self.eta_expand(cx.lvl(), self.eval(&cx, &t), fty)
                    }
                    None => {
                        self.report(
                            self.error("E1255", i.span, format!("missing method `{short}`"))
                                .with_note("this method has no default definition"),
                        );
                        Term::Error
                    }
                }
            } else {
                let b = self.elab_clauses(&mut cx, &short, &m.clauses, fty, i.span);
                self.finish(&cx, &b)
            };
            let body = wrap_lams(binders.clone(), body);
            // Without a runtime argument (a context), the method is a value
            // shared by all the uses of the implementation.
            let body = if binders.iter().any(|b| b.1 != Mult::Zero) {
                cx.holders.clear();
                body
            } else {
                self.share_top(&mut cx, &ty, body)
            };
            let ok = self.error_count() == m_before;
            let body = if ok {
                self.insert_finalizers(&ty, body, i.span)
            } else {
                body
            };
            let ok = self.error_count() == m_before;
            self.prog.globals[m.def.0 as usize] = Global::Def {
                ty,
                body: ok.then_some(body),
            };
            self.prog.order.push(m.def);
        }
        // The dictionary: supertraits, then methods.
        self.begin(name);
        let mut body = Term::Global(info.con);
        for (a, _) in &args {
            body = Term::App(Rc::new(body), Rc::new(self.quote(&cx, a)), Icit::Impl);
        }
        for goal in &ftys[..nsup] {
            let d = match self.search(&cx, goal, 0) {
                Search::Found(t) => t,
                _ => {
                    self.report(
                        self.error(
                            "E1253",
                            i.span,
                            format!("no instance for `{}`", self.show(&cx, goal)),
                        )
                        .with_note("it is a supertrait of the trait being implemented"),
                    );
                    Term::Error
                }
            };
            body = Term::App(Rc::new(body), Rc::new(d), Icit::Expl);
        }
        for (j, m) in i.methods.iter().enumerate() {
            let field = if nbind == 0 {
                // An atom: the dictionary is a constant.
                Term::Global(m.def)
            } else {
                // Eta-expanded, so that building the dictionary does no work
                // (a method may refer to the dictionary being built).
                let t = applied(m.def);
                self.eta_expand(cx.lvl(), self.eval(&cx, &t), &ftys[nsup + j])
            };
            body = Term::App(Rc::new(body), Rc::new(field), Icit::Expl);
        }
        let ok = self.error_count() == before;
        let body = wrap_lams(binders, body);
        self.prog.globals[i.name.0 as usize] = Global::Def {
            ty: dty,
            body: ok.then_some(body),
        };
        self.prog.order.push(i.name);
    }
}

fn wrap_lams(binders: Vec<(Name, Mult, Icit, Term)>, body: Term) -> Term {
    binders.into_iter().rev().fold(body, |b, (x, m, i, a)| {
        Term::Lam(x, m, i, Rc::new(a), Rc::new(b))
    })
}
