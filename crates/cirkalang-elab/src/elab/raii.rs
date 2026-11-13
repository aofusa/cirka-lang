//! Finalizers (spec-v0.0.2.md §5.3). A linear binder whose type holds a
//! value with a `Drop` or `Release` implementation, and that a path of its
//! scope does not use, is given up on that path: in an action with
//! `dropAfter` / `releaseAfter` (at the end of the action, so the values
//! bound last are finalized first), elsewhere with `drop` before the rest of
//! the path. The pass runs on the solved, elaborated body of a definition.

use super::*;

/// An application `h a1 … an`: the head and the arguments.
fn spine(t: &Term) -> (&Term, Vec<(&Term, Icit)>) {
    let mut args = Vec::new();
    let mut h = t;
    while let Term::App(f, a, i) = h {
        args.push((&**a, *i));
        h = f;
    }
    args.reverse();
    (h, args)
}

fn rebuild(h: Term, args: Vec<(Term, Icit)>) -> Term {
    args.into_iter()
        .fold(h, |f, (a, i)| Term::App(Rc::new(f), Rc::new(a), i))
}

impl Elab<'_> {
    /// Inserts the finalizers into the body of a top-level definition of
    /// type `ty` (both closed); `span` is used for errors about binders
    /// that cannot be located.
    pub(super) fn insert_finalizers(&self, ty: &Term, body: Term, span: Span) -> Term {
        if self.fin_spans.borrow().is_empty() || matches!(body, Term::Error) {
            return body;
        }
        self.fin_span_default.set(span);
        let mut cx = Cxt::default();
        let tv = self.eval(&cx, ty);
        self.fin_term(&mut cx, &body, &tv)
    }

    /// The binder needs a finalizer when it is not used.
    fn needs_fin(&self, cx: &Cxt, l: u32) -> bool {
        cx.mults[l as usize] == Mult::One
            && self.drop_status(cx, &self.ty_val(cx, l)) == DropStatus::Fin
    }

    // ---- traversal ----

    /// Finalizes the linear binders of `t` (of type `ty`).
    fn fin_term(&self, cx: &mut Cxt, t: &Term, ty: &Val) -> Term {
        match t {
            Term::Lam(x, m, i, a, b) => {
                let av = self.eval(cx, a);
                let l = self.push(cx, x, *m, &av, None, Span::DUMMY, None);
                let bty = self.cod_type(cx, ty, l, b);
                let mut nb = self.fin_term(cx, b, &bty);
                if self.needs_fin(cx, l) {
                    nb = self.consume_param(cx, l, nb, &bty);
                }
                self.pop(cx, false);
                Term::Lam(x.clone(), *m, *i, a.clone(), Rc::new(nb))
            }
            Term::Let(x, m, a, v, b) => {
                let av = self.eval(cx, a);
                let nv = if *m == Mult::Zero {
                    (**v).clone()
                } else {
                    self.fin_term(cx, v, &av)
                };
                let vv = self.eval(cx, v);
                let l = self.push(cx, x, *m, &av, Some(vv), Span::DUMMY, None);
                let mut nb = self.fin_term(cx, b, ty);
                if self.needs_fin(cx, l) {
                    nb = self.consume(cx, l, nb, ty);
                }
                self.pop(cx, false);
                Term::Let(x.clone(), *m, a.clone(), Rc::new(nv), Rc::new(nb))
            }
            Term::App(..) => {
                let (h, args) = spine(t);
                let mut fty = self.rt_type(cx, h);
                let nh = match h {
                    Term::Global(_) | Term::Var(_) => h.clone(),
                    _ => self.fin_term(cx, h, &fty.clone()),
                };
                let mut nargs = Vec::new();
                for (a, i) in args {
                    match self.whnf(&fty) {
                        Val::Pi(_, m, _, dom, c) => {
                            let na = if m == Mult::Zero {
                                a.clone()
                            } else {
                                self.fin_term(cx, a, &dom)
                            };
                            let av = self.eval(cx, a);
                            fty = self.ev().inst(&c, av);
                            nargs.push((na, i));
                        }
                        _ => {
                            fty = Val::Error;
                            nargs.push((a.clone(), i));
                        }
                    }
                }
                rebuild(nh, nargs)
            }
            Term::Case(k) => self.fin_case(cx, k),
            _ => t.clone(),
        }
    }

    /// The type of the body `b` of a lambda of type `ty` whose binder is
    /// at level `l`.
    fn cod_type(&self, cx: &mut Cxt, ty: &Val, l: u32, b: &Term) -> Val {
        match self.whnf(ty) {
            Val::Pi(_, _, _, _, c) => self.ev().inst(&c, Val::var(l)),
            _ => self.rt_type(cx, b),
        }
    }

    fn fin_case(&self, cx: &mut Cxt, k: &Case) -> Term {
        let Term::Var(si) = &k.scrut else {
            return Term::Case(Rc::new(k.clone()));
        };
        let sl = cx.ix(*si);
        // The fields of a linear value matched by a branch that does not
        // use the value itself belong to the branch.
        let lin = cx.mults[sl as usize] == Mult::One;
        let mut alts = Vec::new();
        for alt in &k.alts {
            let mut bcx = cx.clone();
            let (flvls, rt) = self.enter_alt(&mut bcx, k, sl, alt);
            let (body, rebuild_alt): (&Term, Box<dyn Fn(Term) -> Alt>) = match alt {
                Alt::Con {
                    con,
                    fields,
                    refine,
                    body,
                } => (
                    body,
                    Box::new(move |b| Alt::Con {
                        con: *con,
                        fields: fields.clone(),
                        refine: refine.clone(),
                        body: b,
                    }),
                ),
                Alt::Lit { lit, body } => (
                    body,
                    Box::new(move |b| Alt::Lit {
                        lit: lit.clone(),
                        body: b,
                    }),
                ),
            };
            let mut nb = self.fin_term(&mut bcx, body, &rt);
            if lin && !self.occurs_rt(&mut bcx, sl, &nb) {
                for &l in flvls.iter().rev() {
                    if self.needs_fin(&bcx, l) {
                        nb = self.consume(&mut bcx, l, nb, &rt);
                    }
                }
            }
            alts.push(rebuild_alt(nb));
        }
        let default = k.default.as_ref().map(|d| {
            let rt = self.eval(cx, &k.ret);
            self.fin_term(cx, d, &rt)
        });
        Term::Case(Rc::new(Case {
            scrut: k.scrut.clone(),
            ret: k.ret.clone(),
            alts,
            default,
        }))
    }

    /// Binds the fields of a branch in `bcx` (with the refinements of
    /// dependent matching, as in Lint): their levels and the type of the
    /// branch.
    fn enter_alt(&self, bcx: &mut Cxt, k: &Case, sl: u32, alt: &Alt) -> (Vec<u32>, Val) {
        let base = bcx.lvl() as usize;
        match alt {
            Alt::Con {
                con,
                fields,
                refine,
                ..
            } => {
                let mut flvls = Vec::new();
                for (x, m, fty) in fields {
                    let fv = self.eval(bcx, fty);
                    flvls.push(self.push(bcx, x, *m, &fv, None, Span::DUMMY, None));
                }
                for (ix, t) in refine {
                    let l = bcx.ix(*ix);
                    let v = self.eval(bcx, t);
                    bcx.env[l as usize] = v;
                }
                let sty = self.whnf(&self.ty_val(bcx, sl));
                if let (Val::Data(_, args), Global::Con { nparams, .. }) =
                    (&sty, self.prog.get(*con))
                {
                    let mut vals: Vec<Val> =
                        args.iter().take(*nparams).map(|a| a.0.clone()).collect();
                    vals.extend(flvls.iter().map(|l| Val::var(*l)));
                    bcx.env[sl as usize] = Val::Con(*con, vals);
                }
                let env: Env = bcx.env[..base].to_vec();
                (flvls, self.ev().eval(&env, &k.ret))
            }
            Alt::Lit { lit, .. } => {
                bcx.env[sl as usize] = Val::Lit(lit.clone());
                (Vec::new(), self.eval(bcx, &k.ret))
            }
        }
    }

    // ---- consuming a binder on every path ----

    /// The body of a function whose parameter `x` needs a finalizer: the
    /// parameters that follow are received first (so the function keeps
    /// its arity).
    fn consume_param(&self, cx: &mut Cxt, x: u32, t: Term, ty: &Val) -> Term {
        match t {
            Term::Lam(y, m, i, a, b) => {
                let av = self.eval(cx, &a);
                let l = self.push(cx, &y, m, &av, None, Span::DUMMY, None);
                let bty = self.cod_type(cx, ty, l, &b);
                let nb = self.consume_param(cx, x, (*b).clone(), &bty);
                self.pop(cx, false);
                Term::Lam(y, m, i, a, Rc::new(nb))
            }
            t => self.consume(cx, x, t, ty),
        }
    }

    /// Makes `t` (of type `ty`) use the linear binder `x` on every path,
    /// finalizing it on the paths that do not.
    fn consume(&self, cx: &mut Cxt, x: u32, t: Term, ty: &Val) -> Term {
        if !self.occurs_rt(cx, x, &t) {
            return self.finalize(cx, x, t, ty);
        }
        match t {
            // A closure that captures `x` owns it (it is used at most once:
            // spec-v0.0.2.md §5.1): finalized on its paths when called.
            Term::Lam(y, m, i, a, b) => {
                let av = self.eval(cx, &a);
                let l = self.push(cx, &y, m, &av, None, Span::DUMMY, None);
                let bty = self.cod_type(cx, ty, l, &b);
                let nb = self.consume(cx, x, (*b).clone(), &bty);
                self.pop(cx, false);
                Term::Lam(y, m, i, a, Rc::new(nb))
            }
            Term::Let(y, m, a, v, b) => {
                let av = self.eval(cx, &a);
                if m != Mult::Zero && self.occurs_rt(cx, x, &v) {
                    let nv = self.consume(cx, x, (*v).clone(), &av);
                    return Term::Let(y, m, a, Rc::new(nv), b);
                }
                let vv = self.eval(cx, &v);
                self.push(cx, &y, m, &av, Some(vv), Span::DUMMY, None);
                let nb = self.consume(cx, x, (*b).clone(), ty);
                self.pop(cx, false);
                Term::Let(y, m, a, v, Rc::new(nb))
            }
            Term::App(..) => {
                let (h, args) = spine(&t);
                let hty = self.rt_type(cx, h);
                if !matches!(h, Term::Global(_) | Term::Var(_)) && self.occurs_rt(cx, x, h) {
                    let nh = self.consume(cx, x, h.clone(), &hty);
                    let args = args.into_iter().map(|(a, i)| (a.clone(), i)).collect();
                    return rebuild(nh, args);
                }
                let mut fty = hty;
                let mut nargs = Vec::new();
                let mut done = false;
                for (a, i) in args {
                    let mut na = a.clone();
                    if let Val::Pi(_, m, _, dom, c) = self.whnf(&fty) {
                        if !done
                            && m != Mult::Zero
                            && !matches!(a, Term::Var(_))
                            && self.occurs_rt(cx, x, a)
                        {
                            na = self.consume(cx, x, a.clone(), &dom);
                            done = true;
                        }
                        let av = self.eval(cx, a);
                        fty = self.ev().inst(&c, av);
                    } else {
                        fty = Val::Error;
                    }
                    nargs.push((na, i));
                }
                rebuild(h.clone(), nargs)
            }
            Term::Case(k) => {
                if k.scrut == Term::Var(cx.ix(x)) {
                    return Term::Case(k);
                }
                let Term::Var(si) = &k.scrut else {
                    return Term::Case(k);
                };
                let sl = cx.ix(*si);
                let mut alts = Vec::new();
                for alt in &k.alts {
                    let mut bcx = cx.clone();
                    let (_, rt) = self.enter_alt(&mut bcx, &k, sl, alt);
                    alts.push(match alt {
                        Alt::Con {
                            con,
                            fields,
                            refine,
                            body,
                        } => Alt::Con {
                            con: *con,
                            fields: fields.clone(),
                            refine: refine.clone(),
                            body: self.consume(&mut bcx, x, body.clone(), &rt),
                        },
                        Alt::Lit { lit, body } => Alt::Lit {
                            lit: lit.clone(),
                            body: self.consume(&mut bcx, x, body.clone(), &rt),
                        },
                    });
                }
                let default = k.default.as_ref().map(|d| {
                    let rt = self.eval(cx, &k.ret);
                    self.consume(cx, x, d.clone(), &rt)
                });
                Term::Case(Rc::new(Case {
                    scrut: k.scrut.clone(),
                    ret: k.ret.clone(),
                    alts,
                    default,
                }))
            }
            t => t,
        }
    }

    /// `t` does not use `x`: finalize it there.
    fn finalize(&self, cx: &mut Cxt, x: u32, t: Term, ty: &Val) -> Term {
        let b = &self.prog.builtins;
        let xty = self.ty_val(cx, x);
        let xt = self.quote(cx, &xty);
        let var = Term::Var(cx.ix(x));
        let dict = |tr: Option<GName>| {
            let tr = tr?;
            let goal = Val::Data(tr, vec![(xty.clone(), Icit::Expl)]);
            match self.search(cx, &goal, 0) {
                Search::Found(d) => Some(d),
                _ => None,
            }
        };
        let drop_d = dict(b.drop_trait);
        if let Some(lin) = self.io_kind(ty) {
            // At the end of the action.
            let r = self.quote(cx, &self.io_result(ty));
            let (helper, d) = match drop_d {
                Some(d) => (if lin { b.drop_after1 } else { b.drop_after }, d),
                None => match dict(b.release_trait) {
                    Some(d) => (
                        if lin {
                            b.release_after1
                        } else {
                            b.release_after
                        },
                        d,
                    ),
                    None => {
                        self.report_no_fin(cx, x, &xty, false);
                        return t;
                    }
                },
            };
            let Some(helper) = helper else {
                return t;
            };
            return self.apply_global(helper, &[xt, r, d], &[var, t]);
        }
        // Before the rest of the path.
        match (drop_d, b.drop_method, b.unit) {
            (Some(d), Some(drop), Some(unit)) => {
                let call = self.apply_global(drop, &[xt, d], &[var]);
                Term::Let(
                    "drop$".into(),
                    Mult::Many,
                    Rc::new(Term::Global(unit)),
                    Rc::new(call),
                    Rc::new(shift(&t, 0, 1)),
                )
            }
            _ => {
                let release = dict(b.release_trait).is_some();
                self.report_no_fin(cx, x, &xty, release);
                t
            }
        }
    }

    /// E0307 (only `Release`, outside an action) or E0302.
    fn report_no_fin(&self, cx: &Cxt, x: u32, xty: &Val, release: bool) {
        let name = cx.names[x as usize].clone();
        let span = self
            .fin_spans
            .borrow()
            .iter()
            .find(|(n, _)| *n == name)
            .map(|p| p.1)
            .unwrap_or(self.fin_span_default.get());
        let d = if release {
            self.error(
                "E0307",
                span,
                format!(
                    "linear value `{name}` is left unused, and it can only be released in an action"
                ),
            )
            .with_note(format!(
                "its type `{}` has a `Release` implementation but no `Drop` implementation",
                self.show(cx, xty)
            ))
            .with_help("use it on every path, or release it explicitly in an action with `release`")
        } else {
            self.error(
                "E0302",
                span,
                format!("linear value `{name}` is not consumed"),
            )
            .with_note(format!(
                "its type `{}` has no `Drop` or `Release` implementation",
                self.show(cx, xty)
            ))
            .with_help("consume it by pattern matching or by passing it to a function")
        };
        self.report(d);
    }

    // ---- runtime occurrences and types ----

    /// Whether `t` uses the variable at level `l` at runtime (erased
    /// arguments and types do not count).
    fn occurs_rt(&self, cx: &mut Cxt, l: u32, t: &Term) -> bool {
        term_mentions(t, cx.ix(l)) && self.occ_go(cx, l, t)
    }

    fn occ_go(&self, cx: &mut Cxt, l: u32, t: &Term) -> bool {
        match t {
            Term::Var(i) => cx.ix(*i) == l,
            Term::Lam(x, m, _, a, b) => {
                let av = self.eval(cx, a);
                self.push(cx, x, *m, &av, None, Span::DUMMY, None);
                let r = self.occ_go(cx, l, b);
                self.pop(cx, false);
                r
            }
            Term::Let(x, m, a, v, b) => {
                if *m != Mult::Zero && self.occ_go(cx, l, v) {
                    return true;
                }
                let av = self.eval(cx, a);
                let vv = self.eval(cx, v);
                self.push(cx, x, *m, &av, Some(vv), Span::DUMMY, None);
                let r = self.occ_go(cx, l, b);
                self.pop(cx, false);
                r
            }
            Term::App(..) => {
                let (h, args) = spine(t);
                if !matches!(h, Term::Global(_)) && self.occ_go(cx, l, h) {
                    return true;
                }
                let mut fty = self.rt_type(cx, h);
                for (a, _) in args {
                    match self.whnf(&fty) {
                        Val::Pi(_, m, _, _, c) => {
                            if m != Mult::Zero && self.occ_go(cx, l, a) {
                                return true;
                            }
                            let av = self.eval(cx, a);
                            fty = self.ev().inst(&c, av);
                        }
                        // Not known: count it.
                        _ => {
                            if self.occ_go(cx, l, a) {
                                return true;
                            }
                            fty = Val::Error;
                        }
                    }
                }
                false
            }
            Term::Case(k) => {
                if self.occ_go(cx, l, &k.scrut) {
                    return true;
                }
                let Term::Var(si) = &k.scrut else {
                    return false;
                };
                let sl = cx.ix(*si);
                for alt in &k.alts {
                    let mut bcx = cx.clone();
                    self.enter_alt(&mut bcx, k, sl, alt);
                    let body = match alt {
                        Alt::Con { body, .. } | Alt::Lit { body, .. } => body,
                    };
                    if self.occ_go(&mut bcx, l, body) {
                        return true;
                    }
                }
                k.default.as_ref().is_some_and(|d| self.occ_go(cx, l, d))
            }
            _ => false,
        }
    }

    /// The type of an elaborated term (as in lowering).
    fn rt_type(&self, cx: &mut Cxt, t: &Term) -> Val {
        let b = &self.prog.builtins;
        let prim = |g: Option<GName>| {
            g.map(|g| Val::Neutral(Head::Global(g), Vec::new()))
                .unwrap_or(Val::Error)
        };
        match t {
            Term::Var(i) => self.ty_val(cx, cx.ix(*i)),
            Term::Global(g) => self.global_type(*g).unwrap_or(Val::Error),
            Term::Lit(l) => match l {
                Lit::Int(_) => prim(b.int),
                Lit::UInt(_) => prim(b.uint),
                Lit::Float(_) => prim(b.float),
                Lit::Char(_) => prim(b.char_),
                Lit::Str(_) => prim(b.string),
            },
            Term::Type | Term::LType | Term::Pi(..) => Val::Type,
            Term::Lam(x, m, i, a, body) => {
                let av = self.eval(cx, a);
                let l = self.push(cx, x, *m, &av, None, Span::DUMMY, None);
                let bt = self.rt_type(cx, body);
                let bq = self.ev().quote(l + 1, &bt);
                self.pop(cx, false);
                Val::Pi(
                    x.clone(),
                    *m,
                    *i,
                    Rc::new(av),
                    Closure {
                        env: cx.env.clone(),
                        body: Rc::new(bq),
                    },
                )
            }
            Term::App(f, a, _) => match self.whnf(&self.rt_type(cx, f)) {
                Val::Pi(_, _, _, _, c) => {
                    let av = self.eval(cx, a);
                    self.ev().inst(&c, av)
                }
                _ => Val::Error,
            },
            Term::Let(x, m, a, v, body) => {
                let av = self.eval(cx, a);
                let vv = self.eval(cx, v);
                self.push(cx, x, *m, &av, Some(vv), Span::DUMMY, None);
                let r = self.rt_type(cx, body);
                self.pop(cx, false);
                r
            }
            Term::Case(k) => self.eval(cx, &k.ret),
            _ => Val::Error,
        }
    }
}
