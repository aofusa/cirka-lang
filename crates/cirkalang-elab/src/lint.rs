//! TT Lint: an independent type checker for elaborated TT. It re-checks
//! every definition (types, applications, dependent pattern matching with
//! the recorded index refinements) and that erased (multiplicity 0)
//! variables are not used at runtime. A failure is an internal compiler
//! error.

use crate::eval::*;
use crate::tt::*;
use std::cell::{Cell, RefCell};

#[derive(Clone, Default)]
struct Cx {
    env: Env,
    tys: Vec<Term>,
    mults: Vec<Mult>,
    names: Vec<Name>,
}

impl Cx {
    fn lvl(&self) -> u32 {
        self.env.len() as u32
    }
}

struct Lint<'a> {
    p: &'a Program,
    metas: Metas,
    fuel: Cell<u64>,
    out: Cell<bool>,
    errs: RefCell<Vec<String>>,
    cur: RefCell<String>,
}

/// Checks the whole program.
pub fn lint_program(p: &Program) -> Result<(), Vec<String>> {
    let l = Lint {
        p,
        metas: Metas::default(),
        fuel: Cell::new(FUEL),
        out: Cell::new(false),
        errs: RefCell::new(Vec::new()),
        cur: RefCell::new(String::new()),
    };
    for g in &p.order {
        *l.cur.borrow_mut() = p.name(*g).to_string();
        l.fuel.set(FUEL);
        match p.get(*g) {
            Global::Def { ty, body } => {
                let mut cx = Cx::default();
                l.check_type(&mut cx, ty);
                if let Some(b) = body {
                    let tv = l.ev().eval(&Vec::new(), ty);
                    l.check(&mut cx, b, &tv, Mult::One);
                }
            }
            Global::Data { ty, .. }
            | Global::Con { ty, .. }
            | Global::Prim { ty, .. }
            | Global::Extern { ty, .. } => {
                l.check_type(&mut Cx::default(), ty);
            }
            Global::Missing => {}
        }
    }
    let errs = l.errs.take();
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

impl<'a> Lint<'a> {
    fn ev(&self) -> Ev<'_> {
        Ev::new(self.p, &self.metas, &self.fuel, &self.out)
    }

    fn err(&self, cx: &Cx, msg: String) {
        let _ = cx;
        let m = format!("in `{}`: {msg}", self.cur.borrow());
        self.errs.borrow_mut().push(m);
    }

    fn show(&self, cx: &Cx, v: &Val) -> String {
        let t = self.ev().quote(cx.lvl(), v);
        print_term(self.p, &mut cx.names.clone(), &t)
    }

    fn ty_val(&self, cx: &Cx, l: u32) -> Val {
        let env: Env = cx.env[..l as usize].to_vec();
        self.ev().eval(&env, &cx.tys[l as usize])
    }

    fn push(&self, cx: &mut Cx, x: &Name, m: Mult, ty: &Val, val: Option<Val>) -> u32 {
        let l = cx.lvl();
        cx.tys.push(self.ev().quote(l, ty));
        cx.env.push(val.unwrap_or(Val::var(l)));
        cx.mults.push(m);
        cx.names.push(x.clone());
        l
    }

    fn pop(&self, cx: &mut Cx) {
        cx.env.pop();
        cx.tys.pop();
        cx.mults.pop();
        cx.names.pop();
    }

    fn conv(&self, cx: &Cx, expected: &Val, actual: &Val, what: &str) {
        let ev = self.ev();
        if ev.sub(cx.lvl(), expected, actual, true).is_err() {
            self.err(
                cx,
                format!(
                    "{what}: expected `{}`, found `{}`",
                    self.show(cx, expected),
                    self.show(cx, actual)
                ),
            );
        }
    }

    fn check_type(&self, cx: &mut Cx, t: &Term) {
        let s = self.infer(cx, t, Mult::Zero);
        match self.ev().whnf(s) {
            Val::Type | Val::LType => {}
            other => self.err(
                cx,
                format!(
                    "expected a type, found a term of type `{}`",
                    self.show(cx, &other)
                ),
            ),
        }
    }

    fn check(&self, cx: &mut Cx, t: &Term, ty: &Val, rig: Mult) {
        let a = self.infer(cx, t, rig);
        self.conv(cx, ty, &a, "type mismatch");
    }

    fn lit_type(&self, l: &Lit) -> Val {
        let b = &self.p.builtins;
        let g = match l {
            Lit::Int(_) => b.int,
            Lit::UInt(_) => b.uint,
            Lit::Float(_) => b.float,
            Lit::Char(_) => b.char_,
            Lit::Str(_) => b.string,
        };
        match g {
            Some(g) => Val::Neutral(Head::Global(g), Vec::new()),
            None => Val::Error,
        }
    }

    fn infer(&self, cx: &mut Cx, t: &Term, rig: Mult) -> Val {
        let inner = if rig == Mult::Zero {
            Mult::Zero
        } else {
            Mult::One
        };
        match t {
            Term::Var(i) => {
                if *i >= cx.lvl() {
                    self.err(cx, format!("variable #{i} out of scope"));
                    return Val::Error;
                }
                let l = cx.lvl() - 1 - i;
                if rig != Mult::Zero && cx.mults[l as usize] == Mult::Zero {
                    self.err(
                        cx,
                        format!("erased variable `{}` used at runtime", cx.names[l as usize]),
                    );
                }
                self.ty_val(cx, l)
            }
            Term::Global(g) => match self.p.type_of(*g) {
                Some(ty) => self.ev().eval(&Vec::new(), ty),
                None => {
                    self.err(
                        cx,
                        format!("reference to missing global `{}`", self.p.name(*g)),
                    );
                    Val::Error
                }
            },
            Term::Type | Term::LType => Val::Type,
            Term::Lit(l) => self.lit_type(l),
            Term::Pi(x, m, _, a, b) => {
                self.check_type(cx, a);
                let av = self.ev().eval(&cx.env, a);
                self.push(cx, x, *m, &av, None);
                self.check_type(cx, b);
                self.pop(cx);
                Val::Type
            }
            Term::Lam(x, m, i, a, b) => {
                self.check_type(cx, a);
                let av = self.ev().eval(&cx.env, a);
                let l = self.push(cx, x, *m, &av, None);
                let bt = self.infer(cx, b, inner);
                let bq = self.ev().quote(l + 1, &bt);
                self.pop(cx);
                Val::Pi(
                    x.clone(),
                    *m,
                    *i,
                    std::rc::Rc::new(av),
                    Closure {
                        env: cx.env.clone(),
                        body: std::rc::Rc::new(bq),
                    },
                )
            }
            Term::App(f, a, i) => {
                let ft = self.infer(cx, f, rig);
                match self.ev().whnf(ft) {
                    Val::Pi(_, m, i2, dom, c) => {
                        if *i != i2 {
                            self.err(cx, "application with the wrong plicity".into());
                        }
                        self.check(cx, a, &dom, rig.times(m));
                        let av = self.ev().eval(&cx.env, a);
                        self.ev().inst(&c, av)
                    }
                    Val::Error => Val::Error,
                    other => {
                        self.err(
                            cx,
                            format!(
                                "applying a non-function of type `{}`",
                                self.show(cx, &other)
                            ),
                        );
                        Val::Error
                    }
                }
            }
            Term::Let(x, m, a, v, b) => {
                self.check_type(cx, a);
                let av = self.ev().eval(&cx.env, a);
                self.check(cx, v, &av, inner.times(*m));
                let vv = self.ev().eval(&cx.env, v);
                self.push(cx, x, *m, &av, Some(vv));
                let bt = self.infer(cx, b, rig);
                self.pop(cx);
                bt
            }
            Term::Case(c) => self.infer_case(cx, c, rig),
            Term::Meta(_) | Term::InsertedMeta(..) => {
                self.err(cx, "unsolved metavariable".into());
                Val::Error
            }
            Term::Error => {
                self.err(cx, "error term in an elaborated program".into());
                Val::Error
            }
        }
    }

    fn infer_case(&self, cx: &mut Cx, c: &Case, rig: Mult) -> Val {
        let n_branches = c.alts.len() + c.default.is_some() as usize;
        let srig = if n_branches <= 1 { Mult::Zero } else { rig };
        let sty = self.infer(cx, &c.scrut, srig);
        let sty = self.ev().whnf(sty);
        let scrut_lvl = match &c.scrut {
            Term::Var(i) => Some(cx.lvl() - 1 - i),
            _ => None,
        };
        self.check_type(cx, &c.ret);
        let base = cx.lvl() as usize;
        for alt in &c.alts {
            let mut bcx = cx.clone();
            match alt {
                Alt::Con {
                    con,
                    fields,
                    refine,
                    body,
                } => {
                    let Val::Data(d, args) = &sty else {
                        self.err(
                            cx,
                            format!(
                                "constructor match on a value of type `{}`",
                                self.show(cx, &sty)
                            ),
                        );
                        continue;
                    };
                    let Global::Con {
                        ty, data, nparams, ..
                    } = self.p.get(*con)
                    else {
                        self.err(cx, "match on a non-constructor".into());
                        continue;
                    };
                    if data != d {
                        self.err(
                            cx,
                            format!(
                                "constructor `{}` does not belong to `{}`",
                                self.p.name(*con),
                                self.p.name(*d)
                            ),
                        );
                        continue;
                    }
                    let params: Vec<Val> =
                        args.iter().take(*nparams).map(|a| a.0.clone()).collect();
                    let mut ct = self.ev().eval(&Vec::new(), ty);
                    for p in &params {
                        if let Val::Pi(_, _, _, _, clo) = self.ev().whnf(ct.clone()) {
                            ct = self.ev().inst(&clo, p.clone());
                        }
                    }
                    let mut fvars = Vec::new();
                    for (x, m, fty) in fields {
                        self.check_type(&mut bcx, fty);
                        let fv = self.ev().eval(&bcx.env, fty);
                        match self.ev().whnf(ct.clone()) {
                            Val::Pi(_, _, _, a, clo) => {
                                self.conv(&bcx, &a, &fv, "field type");
                                let l = self.push(&mut bcx, x, *m, &fv, None);
                                fvars.push(Val::var(l));
                                ct = self.ev().inst(&clo, Val::var(l));
                            }
                            _ => {
                                self.err(
                                    &bcx,
                                    format!("too many fields for `{}`", self.p.name(*con)),
                                );
                                break;
                            }
                        }
                    }
                    // Indices before refinement (as terms), then refine.
                    let idx: Vec<(Term, Term)> = match self.ev().whnf(ct.clone()) {
                        Val::Data(_, rargs) => rargs
                            .iter()
                            .zip(args.iter())
                            .skip(*nparams)
                            .map(|(a, b)| {
                                (
                                    self.ev().quote(bcx.lvl(), &a.0),
                                    self.ev().quote(bcx.lvl(), &b.0),
                                )
                            })
                            .collect(),
                        _ => {
                            self.err(&bcx, "constructor does not return its data type".into());
                            Vec::new()
                        }
                    };
                    for (ix, t) in refine {
                        let l = bcx.lvl() - 1 - ix;
                        let v = self.ev().eval(&bcx.env, t);
                        bcx.env[l as usize] = v;
                    }
                    if let Some(sl) = scrut_lvl {
                        let mut args2 = params.clone();
                        args2.extend(fvars);
                        bcx.env[sl as usize] = Val::Con(*con, args2);
                    }
                    for (a, b) in idx {
                        let va = self.ev().eval(&bcx.env, &a);
                        let vb = self.ev().eval(&bcx.env, &b);
                        self.conv(&bcx, &vb, &va, "index refinement");
                    }
                    let env: Env = bcx.env[..base].to_vec();
                    let rt = self.ev().eval(&env, &c.ret);
                    self.check(&mut bcx, body, &rt, rig);
                }
                Alt::Lit { lit, body } => {
                    self.conv(cx, &sty, &self.lit_type(lit), "literal pattern");
                    if let Some(sl) = scrut_lvl {
                        bcx.env[sl as usize] = Val::Lit(lit.clone());
                    }
                    let rt = self.ev().eval(&bcx.env, &c.ret);
                    self.check(&mut bcx, body, &rt, rig);
                }
            }
        }
        if let Some(d) = &c.default {
            let rt = self.ev().eval(&cx.env, &c.ret);
            self.check(cx, d, &rt, rig);
        }
        self.ev().eval(&cx.env, &c.ret)
    }
}
