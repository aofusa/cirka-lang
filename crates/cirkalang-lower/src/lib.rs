//! cirkalang-lower: erasure of elaborated TT and translation into the
//! common Core (spec-v0.0.1.md §6).
//!
//! - Multiplicity-0 binders, arguments and fields disappear. Erased binders
//!   whose type is a kind become Core `forall`s / type applications.
//! - Data types keep only the parameters that are types; value indices are
//!   dropped. Types that Core cannot express (computed from values) become
//!   `Any`, and `cast`s are inserted where the Core types disagree.
//! - Evaluation is strict: TT `let`s become `let!`, and arguments and
//!   constructor fields are evaluated by `let!` before the call.
//! - All Core multiplicities are ω: linearity was checked on TT, where the
//!   implicit `Drop` of v0.0.1 makes unused linear values legal.

use cirka_core as c;
use cirka_diagnostics::{Diagnostic, Span};
use cirkalang_elab::eval::{Ev, FUEL, Metas};
use cirkalang_elab::tt::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

mod externs;
#[cfg(test)]
mod tests;

const SUPPORT: &str = include_str!("support.core");

/// Lowers an elaborated program to Core. The result passes Core Lint (a
/// failure is an internal compiler error).
pub fn lower(p: &Program) -> Result<c::Program, Vec<Diagnostic>> {
    let l = Lower::new(p);
    let prog = l.program()?;
    let errs = c::lint::lint_program(&prog);
    if !errs.is_empty() {
        let mut out = Vec::new();
        for e in errs {
            out.push(Diagnostic::ice(format!(
                "Core Lint after lowering: {}",
                e.message
            )));
        }
        return Err(out);
    }
    Ok(prog)
}

struct DataInfo {
    name: String,
    /// Which TT parameters are Core type parameters.
    tparams: Vec<bool>,
    tvs: Vec<c::TyVar>,
}

struct ConInfo {
    name: String,
    data: GName,
    /// Which TT fields exist at runtime.
    runtime: Vec<bool>,
    /// Core types of the runtime fields (over the data's type variables).
    field_tys: Vec<c::Type>,
}

#[derive(Clone, Default)]
struct Cx {
    env: Env,
    tys: Vec<Term>,
    tvs: Vec<Option<c::TyVar>>,
    vars: Vec<Option<(c::Id, c::Type)>>,
}

impl Cx {
    fn lvl(&self) -> u32 {
        self.env.len() as u32
    }
}

struct Lower<'a> {
    p: &'a Program,
    metas: Metas,
    fuel: Cell<u64>,
    out: Cell<bool>,
    next: Cell<u32>,
    datas: RefCell<HashMap<GName, DataInfo>>,
    cons: RefCell<HashMap<GName, ConInfo>>,
    errs: RefCell<Vec<String>>,
    /// Errors of the program found while lowering (extern functions
    /// declared twice with different signatures).
    user_errs: RefCell<Vec<Diagnostic>>,
}

type Binds = Vec<(c::Binder, c::Expr)>;

fn wrap(binds: Binds, body: c::Expr) -> c::Expr {
    binds.into_iter().rev().fold(body, |b, (x, e)| {
        c::Expr::LetStrict(x, Box::new(e), Box::new(b))
    })
}

fn app(f: c::Expr, args: Vec<c::Expr>) -> c::Expr {
    c::Expr::app(f, args)
}

fn prim(op: c::PrimOp, args: Vec<c::Expr>) -> c::Expr {
    c::Expr::Prim(op, Vec::new(), args)
}

fn int_ty() -> c::Type {
    c::Type::con("Int")
}

/// `I# e`: an `Int` from an `Int#`.
fn int_con(e: c::Expr) -> c::Expr {
    c::Expr::Con("I#".into(), Vec::new(), vec![e])
}

/// The element type of the Core type `Array a` (`Any` otherwise).
fn array_elem(t: &c::Type) -> c::Type {
    match t {
        c::Type::Con(n, args) if n == c::ARRAY && args.len() == 1 => args[0].clone(),
        _ => c::Type::Any,
    }
}

/// `t` with `IO a` replaced by its representation `Unit# -> a`.
fn io_rep(t: &c::Type) -> c::Type {
    match t {
        c::Type::Con(n, args) if n == "IO" && args.len() == 1 => {
            c::Type::fun(c::Mult::Many, c::Type::unit(), io_rep(&args[0]))
        }
        c::Type::Con(n, args) => c::Type::Con(n.clone(), args.iter().map(io_rep).collect()),
        c::Type::App(h, args) => {
            c::Type::App(Box::new(io_rep(h)), args.iter().map(io_rep).collect())
        }
        c::Type::Fun(m, a, b) => c::Type::Fun(*m, Box::new(io_rep(a)), Box::new(io_rep(b))),
        c::Type::Forall(v, k, b) => c::Type::Forall(v.clone(), k.clone(), Box::new(io_rep(b))),
        t => t.clone(),
    }
}

/// `e` (of a type with the same quantifiers as `ty`) cast to `ty`.
fn cast_poly(e: c::Expr, ty: &c::Type) -> c::Expr {
    match ty {
        c::Type::Forall(v, k, b) => {
            let inner = cast_poly(c::Expr::TyApp(Box::new(e), c::Type::Var(v.clone())), b);
            c::Expr::TyLam(v.clone(), k.clone(), Box::new(inner))
        }
        t => c::Expr::Cast(Box::new(e), t.clone()),
    }
}

impl<'a> Lower<'a> {
    fn new(p: &'a Program) -> Lower<'a> {
        Lower {
            p,
            metas: Metas::default(),
            fuel: Cell::new(FUEL),
            out: Cell::new(false),
            next: Cell::new(1),
            datas: RefCell::new(HashMap::new()),
            cons: RefCell::new(HashMap::new()),
            errs: RefCell::new(Vec::new()),
            user_errs: RefCell::new(Vec::new()),
        }
    }

    fn ev(&self) -> Ev<'_> {
        Ev::new(self.p, &self.metas, &self.fuel, &self.out)
    }

    fn whnf(&self, v: &Val) -> Val {
        self.ev().whnf(v.clone())
    }

    fn fresh(&self) -> u32 {
        let n = self.next.get();
        self.next.set(n + 1);
        n
    }

    fn id(&self, name: &str) -> c::Id {
        let clean: String = name.chars().filter(|c| !c.is_whitespace()).collect();
        c::Id::new(
            if clean.is_empty() || clean == "_" {
                "x".to_string()
            } else {
                clean
            },
            self.fresh(),
        )
    }

    fn tv(&self, name: &str) -> c::TyVar {
        c::TyVar::new(if name == "_" { "t" } else { name }, self.fresh())
    }

    fn ice(&self, msg: String) {
        self.errs.borrow_mut().push(msg);
    }

    fn gname(&self, g: GName) -> String {
        self.p.name(g).to_string()
    }

    // ---- types ----

    /// The Core kind of a TT type that is a kind (`Type`, `Type -> Type`, ...).
    fn core_kind(&self, v: &Val) -> Option<c::Kind> {
        match self.whnf(v) {
            Val::Type | Val::LType => Some(c::Kind::Type),
            Val::Pi(_, _, _, a, clo) => {
                let ka = self.core_kind(&a)?;
                let kb = self.core_kind(&self.ev().inst(&clo, Val::var(1 << 24)))?;
                Some(c::Kind::Arrow(Box::new(ka), Box::new(kb)))
            }
            _ => None,
        }
    }

    fn is_universe_valued(&self, v: &Val) -> bool {
        match self.whnf(v) {
            Val::Type | Val::LType => true,
            Val::Pi(_, _, _, _, clo) => {
                self.is_universe_valued(&self.ev().inst(&clo, Val::var(1 << 24)))
            }
            _ => false,
        }
    }

    fn lower_ty(&self, tvs: &mut Vec<Option<c::TyVar>>, v: &Val) -> c::Type {
        match self.whnf(v) {
            Val::Pi(x, m, _, a, clo) => {
                let l = tvs.len() as u32;
                let body = self.ev().inst(&clo, Val::var(l));
                if m == Mult::Zero {
                    if let Some(k) = self.core_kind(&a) {
                        let tv = self.tv(&x);
                        tvs.push(Some(tv.clone()));
                        let b = self.lower_ty(tvs, &body);
                        tvs.pop();
                        c::Type::Forall(tv, k, Box::new(b))
                    } else {
                        tvs.push(None);
                        let b = self.lower_ty(tvs, &body);
                        tvs.pop();
                        b
                    }
                } else {
                    let a2 = self.lower_ty(tvs, &a);
                    tvs.push(None);
                    let b = self.lower_ty(tvs, &body);
                    tvs.pop();
                    c::Type::fun(c::Mult::Many, a2, b)
                }
            }
            Val::Data(d, args) => {
                let datas = self.datas.borrow();
                let Some(info) = datas.get(&d) else {
                    return c::Type::Any;
                };
                let name = info.name.clone();
                let mask = info.tparams.clone();
                drop(datas);
                let mut targs = Vec::new();
                for (i, (a, _)) in args.iter().enumerate() {
                    if mask.get(i).copied().unwrap_or(false) {
                        targs.push(self.lower_ty(tvs, a));
                    }
                }
                c::Type::Con(name, targs)
            }
            Val::Neutral(Head::Var(l), sp) => match tvs.get(l as usize).cloned() {
                Some(Some(tv)) => {
                    let mut args = Vec::new();
                    for e in &sp {
                        match e {
                            Elim::App(a, _) => args.push(self.lower_ty(tvs, a)),
                            Elim::Case(..) => return c::Type::Any,
                        }
                    }
                    c::Type::mk_app(c::Type::Var(tv), args)
                }
                _ => c::Type::Any,
            },
            Val::Neutral(Head::Global(g), sp) => {
                let args: Vec<&Val> = sp
                    .iter()
                    .filter_map(|e| match e {
                        Elim::App(a, _) => Some(a),
                        Elim::Case(..) => None,
                    })
                    .collect();
                match (self.p.prim_of(g), args.as_slice()) {
                    (Some(Prim::Int), []) => int_ty(),
                    (Some(Prim::UInt), []) => c::Type::con("UInt"),
                    (Some(Prim::Float), []) => c::Type::con("Float"),
                    (Some(Prim::Char), []) => c::Type::con("Char"),
                    (Some(Prim::String), []) => c::Type::bytes(),
                    // A newtype of `Unit# -> a` (06-ir.md §1.6), so that it
                    // can be the argument of a higher-kinded parameter.
                    // `IO1` has the same representation (only the
                    // multiplicity of the result differs).
                    (Some(Prim::IO | Prim::IO1), []) => c::Type::Con("IO".into(), Vec::new()),
                    (Some(Prim::IO | Prim::IO1), [a]) => {
                        c::Type::Con("IO".into(), vec![self.lower_ty(tvs, a)])
                    }
                    (Some(Prim::Unique), [a]) => self.lower_ty(tvs, a),
                    (Some(Prim::Array), [a]) => {
                        c::Type::Con(c::ARRAY.into(), vec![self.lower_ty(tvs, a)])
                    }
                    // 8-byte words (spec-v0.0.2.md §7.3).
                    (Some(Prim::UArray), [_]) => c::Type::bytes(),
                    // Borrows (spec-v0.0.3.md §2): regions and references
                    // are erased tokens; a borrow is a newtype of a state
                    // function over the owner.
                    (Some(Prim::Region | Prim::Ref), _) => self.unit_ty(),
                    // Regions (spec-v0.0.3.md §3): the arena is the runtime's
                    // region object, a handle the address of a cell, and
                    // `RIO r` is `IO`.
                    // A box with one lazy field (spec-v0.0.3.md §4).
                    (Some(Prim::Lazy), [a]) => {
                        c::Type::Con("Lazy".into(), vec![self.lower_ty(tvs, a)])
                    }
                    (Some(Prim::Arena), _) => c::Type::Any,
                    (Some(Prim::In | Prim::RefIn), _) => int_ty(),
                    (Some(Prim::RIO), [_]) => c::Type::Con("IO".into(), Vec::new()),
                    (Some(Prim::RIO), [_, a]) => {
                        c::Type::Con("IO".into(), vec![self.lower_ty(tvs, a)])
                    }
                    (Some(Prim::Borrow), [a, _, rest @ ..]) => c::Type::Con(
                        "Borrow".into(),
                        std::iter::once(*a)
                            .chain(rest.iter().copied())
                            .map(|t| self.lower_ty(tvs, t))
                            .collect(),
                    ),
                    (Some(Prim::BorrowIO), [a, _, rest @ ..]) => c::Type::Con(
                        "BorrowIO".into(),
                        std::iter::once(*a)
                            .chain(rest.iter().copied())
                            .map(|t| self.lower_ty(tvs, t))
                            .collect(),
                    ),
                    _ => c::Type::Any,
                }
            }
            _ => c::Type::Any,
        }
    }

    fn cx_ty(&self, cx: &Cx, v: &Val) -> c::Type {
        let mut tvs = cx.tvs.clone();
        self.lower_ty(&mut tvs, v)
    }

    // ---- data types ----

    fn data(&self, d: GName, out: &mut Vec<c::DataDecl>) {
        let Global::Data {
            ty, nparams, cons, ..
        } = self.p.get(d)
        else {
            return;
        };
        let mut tparams = Vec::new();
        let mut tvs = Vec::new();
        let mut params = Vec::new();
        let mut v = self.ev().eval(&Vec::new(), ty);
        for k in 0..*nparams {
            let Val::Pi(x, _, _, a, clo) = self.whnf(&v) else {
                break;
            };
            match self.core_kind(&a) {
                Some(kind) => {
                    let tv = self.tv(&x);
                    tparams.push(true);
                    tvs.push(tv.clone());
                    params.push((tv, kind));
                }
                None => tparams.push(false),
            }
            v = self.ev().inst(&clo, Val::var(k as u32));
        }
        self.datas.borrow_mut().insert(
            d,
            DataInfo {
                name: self.gname(d),
                tparams: tparams.clone(),
                tvs: tvs.clone(),
            },
        );
        let _ = cons;
        out.push(c::DataDecl {
            name: self.gname(d),
            params,
            linear: false,
            ctors: Vec::new(),
            newtype: false,
        });
    }

    fn data_ctors(&self, d: GName, decl: &mut c::DataDecl) {
        let Global::Data { nparams, cons, .. } = self.p.get(d) else {
            return;
        };
        let (tparams, dtvs) = {
            let ds = self.datas.borrow();
            let i = &ds[&d];
            (i.tparams.clone(), i.tvs.clone())
        };
        for &k in cons {
            let Global::Con { ty, .. } = self.p.get(k) else {
                continue;
            };
            let mut tvs: Vec<Option<c::TyVar>> = Vec::new();
            let mut v = self.ev().eval(&Vec::new(), ty);
            let mut di = 0;
            for i in 0..*nparams {
                let Val::Pi(_, _, _, _, clo) = self.whnf(&v) else {
                    break;
                };
                if tparams[i] {
                    tvs.push(Some(dtvs[di].clone()));
                    di += 1;
                } else {
                    tvs.push(None);
                }
                v = self.ev().inst(&clo, Val::var(i as u32));
            }
            let mut runtime = Vec::new();
            let mut field_tys = Vec::new();
            while let Val::Pi(_, m, _, a, clo) = self.whnf(&v) {
                let l = tvs.len() as u32;
                if m == Mult::Zero {
                    runtime.push(false);
                } else {
                    runtime.push(true);
                    field_tys.push(self.lower_ty(&mut tvs, &a));
                }
                tvs.push(None);
                v = self.ev().inst(&clo, Val::var(l));
            }
            decl.ctors.push(c::CtorDecl {
                name: self.gname(k),
                // Dictionary fields may refer to the dictionary being built.
                strict: vec![!self.p.traits.contains_key(&d); field_tys.len()],
                fields: field_tys.clone(),
            });
            self.cons.borrow_mut().insert(
                k,
                ConInfo {
                    name: self.gname(k),
                    data: d,
                    runtime,
                    field_tys,
                },
            );
        }
    }

    // ---- TT types of terms ----

    fn ty_val(&self, cx: &Cx, l: u32) -> Val {
        let env: Env = cx.env[..l as usize].to_vec();
        self.ev().eval(&env, &cx.tys[l as usize])
    }

    fn lit_tt_type(&self, l: &Lit) -> Val {
        let b = &self.p.builtins;
        let g = match l {
            Lit::Int(_) => b.int,
            Lit::UInt(_) => b.uint,
            Lit::Float(_) => b.float,
            Lit::Char(_) => b.char_,
            Lit::Str(_) => b.string,
        };
        g.map(|g| Val::Neutral(Head::Global(g), Vec::new()))
            .unwrap_or(Val::Error)
    }

    fn push(
        &self,
        cx: &mut Cx,
        ty: &Val,
        val: Option<Val>,
        tv: Option<c::TyVar>,
        var: Option<(c::Id, c::Type)>,
    ) -> u32 {
        let l = cx.lvl();
        cx.tys.push(self.ev().quote(l, ty));
        cx.env.push(val.unwrap_or(Val::var(l)));
        cx.tvs.push(tv);
        cx.vars.push(var);
        l
    }

    fn pop(&self, cx: &mut Cx) {
        cx.env.pop();
        cx.tys.pop();
        cx.tvs.pop();
        cx.vars.pop();
    }

    fn infer_ty(&self, cx: &mut Cx, t: &Term) -> Val {
        match t {
            Term::Var(i) => self.ty_val(cx, cx.lvl() - 1 - i),
            Term::Global(g) => match self.p.type_of(*g) {
                Some(ty) => self.ev().eval(&Vec::new(), ty),
                None => Val::Error,
            },
            Term::Lit(l) => self.lit_tt_type(l),
            Term::Type | Term::LType | Term::Pi(..) => Val::Type,
            Term::Lam(x, m, i, a, b) => {
                let av = self.ev().eval(&cx.env, a);
                let l = self.push(cx, &av, None, None, None);
                let bt = self.infer_ty(cx, b);
                let bq = self.ev().quote(l + 1, &bt);
                self.pop(cx);
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
            Term::App(f, a, _) => match self.whnf(&self.infer_ty(cx, f)) {
                Val::Pi(_, _, _, _, clo) => {
                    let av = self.ev().eval(&cx.env, a);
                    self.ev().inst(&clo, av)
                }
                _ => Val::Error,
            },
            Term::Let(_, _, a, v, b) => {
                let av = self.ev().eval(&cx.env, a);
                let vv = self.ev().eval(&cx.env, v);
                self.push(cx, &av, Some(vv), None, None);
                let r = self.infer_ty(cx, b);
                self.pop(cx);
                r
            }
            Term::Case(k) => self.ev().eval(&cx.env, &k.ret),
            _ => Val::Error,
        }
    }

    // ---- expressions ----

    fn coerce(&self, e: c::Expr, have: &c::Type, want: Option<&c::Type>) -> (c::Expr, c::Type) {
        match want {
            Some(w) if !have.alpha_eq(w) => (c::Expr::Cast(Box::new(e), w.clone()), w.clone()),
            Some(w) => (e, w.clone()),
            None => (e, have.clone()),
        }
    }

    fn atom(&self, e: c::Expr, ty: &c::Type, binds: &mut Binds) -> c::Expr {
        if e.is_atom() {
            return e;
        }
        let id = self.id("t");
        binds.push((c::Binder::new(id.clone(), c::Mult::Many, ty.clone()), e));
        c::Expr::Var(id)
    }

    fn lit(&self, l: &Lit) -> (c::Expr, c::Type) {
        let boxed = |con: &str, ty: &str, x: c::Lit| {
            (
                c::Expr::Con(con.into(), Vec::new(), vec![c::Expr::Lit(x)]),
                c::Type::con(ty),
            )
        };
        match l {
            Lit::Int(n) => boxed("I#", "Int", c::Lit::Int(*n)),
            Lit::UInt(n) => boxed("W#", "UInt", c::Lit::Int(*n as i64)),
            Lit::Float(x) => boxed("F#", "Float", c::Lit::Double(*x)),
            Lit::Char(ch) => boxed("C#", "Char", c::Lit::Int(*ch as i64)),
            Lit::Str(s) => (
                c::Expr::Lit(c::Lit::Str(s.as_bytes().to_vec())),
                c::Type::bytes(),
            ),
        }
    }

    fn expr(&self, cx: &mut Cx, t: &Term, want: Option<&c::Type>) -> (c::Expr, c::Type) {
        match t {
            Term::Var(i) => {
                let l = cx.lvl() - 1 - i;
                match &cx.vars[l as usize] {
                    Some((id, ty)) => {
                        let ty = ty.clone();
                        self.coerce(c::Expr::Var(id.clone()), &ty, want)
                    }
                    None => {
                        self.ice(format!("erased variable #{l} used at runtime"));
                        (c::Expr::Panic("erased".into(), c::Type::Any), c::Type::Any)
                    }
                }
            }
            Term::Global(_) | Term::App(..) => self.spine(cx, t, want),
            Term::Lit(l) => {
                let (e, ty) = self.lit(l);
                self.coerce(e, &ty, want)
            }
            Term::Lam(x, m, _, a, b) => {
                let av = self.ev().eval(&cx.env, a);
                if *m == Mult::Zero {
                    if let Some(k) = self.core_kind(&av) {
                        let tv = self.tv(x);
                        let inner = match want {
                            Some(c::Type::Forall(tv2, _, body)) => {
                                Some(body.subst(tv2, &c::Type::Var(tv.clone())))
                            }
                            _ => None,
                        };
                        self.push(cx, &av, None, Some(tv.clone()), None);
                        let (be, bt) = self.expr(cx, b, inner.as_ref());
                        self.pop(cx);
                        let ty = c::Type::Forall(tv.clone(), k.clone(), Box::new(bt));
                        return self.coerce(c::Expr::TyLam(tv, k, Box::new(be)), &ty, want);
                    }
                    self.push(cx, &av, None, None, None);
                    let r = self.expr(cx, b, want);
                    self.pop(cx);
                    return r;
                }
                let (dom, cod) = match want {
                    Some(c::Type::Fun(_, a, r)) => ((**a).clone(), Some((**r).clone())),
                    _ => (self.cx_ty(cx, &av), None),
                };
                let id = self.id(x);
                self.push(cx, &av, None, None, Some((id.clone(), dom.clone())));
                let (be, bt) = self.expr(cx, b, cod.as_ref());
                self.pop(cx);
                let ty = c::Type::fun(c::Mult::Many, dom.clone(), bt);
                let e = c::Expr::Lam(c::Binder::new(id, c::Mult::Many, dom), Box::new(be));
                self.coerce(e, &ty, want)
            }
            Term::Let(x, m, a, v, b) => {
                let av = self.ev().eval(&cx.env, a);
                let vv = self.ev().eval(&cx.env, v);
                if *m == Mult::Zero {
                    self.push(cx, &av, Some(vv), None, None);
                    let r = self.expr(cx, b, want);
                    self.pop(cx);
                    return r;
                }
                let ty = self.cx_ty(cx, &av);
                let (ve, _) = self.expr(cx, v, Some(&ty));
                let id = self.id(x);
                self.push(cx, &av, Some(vv), None, Some((id.clone(), ty.clone())));
                let (be, bt) = self.expr(cx, b, want);
                self.pop(cx);
                (
                    c::Expr::LetStrict(
                        c::Binder::new(id, c::Mult::Many, ty),
                        Box::new(ve),
                        Box::new(be),
                    ),
                    bt,
                )
            }
            Term::Case(k) => self.case(cx, k, want),
            Term::Type | Term::LType | Term::Pi(..) => {
                let ty = want.cloned().unwrap_or(c::Type::Any);
                (
                    c::Expr::Panic("a type used at runtime".into(), ty.clone()),
                    ty,
                )
            }
            Term::Meta(_) | Term::InsertedMeta(..) | Term::Error => {
                self.ice("unelaborated term in lowering".into());
                let ty = want.cloned().unwrap_or(c::Type::Any);
                (c::Expr::Panic("internal error".into(), ty.clone()), ty)
            }
        }
    }

    fn spine(&self, cx: &mut Cx, t: &Term, want: Option<&c::Type>) -> (c::Expr, c::Type) {
        let mut args = Vec::new();
        let mut h = t;
        while let Term::App(f, a, _) = h {
            args.push(&**a);
            h = f;
        }
        args.reverse();
        if let Term::Global(g) = h
            && self.cons.borrow().contains_key(g)
        {
            return self.con_app(cx, *g, &args, want);
        }
        let mut binds = Vec::new();
        let (he, hty) = match h {
            Term::Global(g) => {
                let ty = match self.p.type_of(*g) {
                    Some(ty) => {
                        let v = self.ev().eval(&Vec::new(), ty);
                        self.lower_ty(&mut Vec::new(), &v)
                    }
                    None => c::Type::Any,
                };
                (c::Expr::Global(self.gname(*g)), ty)
            }
            _ => self.expr(cx, h, None),
        };
        let mut fe = self.atom(he, &hty, &mut binds);
        let mut fty = hty;
        let mut tt_ty = self.infer_ty(cx, h);
        for a in args {
            let Val::Pi(_, m, _, dom, clo) = self.whnf(&tt_ty) else {
                self.ice("application of a non-function in lowering".into());
                break;
            };
            let av = self.ev().eval(&cx.env, a);
            if m == Mult::Zero {
                if self.core_kind(&dom).is_some()
                    && let c::Type::Forall(tv, _, body) = &fty
                {
                    let ct = self.cx_ty(cx, &av);
                    let nb = body.subst(tv, &ct);
                    fe = c::Expr::TyApp(Box::new(fe), ct);
                    fty = nb;
                }
            } else {
                let (aw, res) = match &fty {
                    c::Type::Fun(_, a, r) => ((**a).clone(), (**r).clone()),
                    other => {
                        // Not a known function type (e.g. `Any`): cast.
                        let f = c::Type::fun(c::Mult::Many, c::Type::Any, c::Type::Any);
                        if !other.alpha_eq(&f) {
                            fe = c::Expr::Cast(Box::new(fe), f);
                        }
                        (c::Type::Any, c::Type::Any)
                    }
                };
                let (ae, _) = self.expr(cx, a, Some(&aw));
                let ae = self.atom(ae, &aw, &mut binds);
                fe = c::Expr::App(Box::new(fe), Box::new(ae));
                fty = res;
            }
            tt_ty = self.ev().inst(&clo, av);
        }
        let (e, ty) = self.coerce(fe, &fty, want);
        (wrap(binds, e), ty)
    }

    fn con_app(
        &self,
        cx: &mut Cx,
        k: GName,
        args: &[&Term],
        want: Option<&c::Type>,
    ) -> (c::Expr, c::Type) {
        let (name, data, runtime, field_tys) = {
            let cs = self.cons.borrow();
            let i = &cs[&k];
            (
                i.name.clone(),
                i.data,
                i.runtime.clone(),
                i.field_tys.clone(),
            )
        };
        let (tparams, dtvs, dname) = {
            let ds = self.datas.borrow();
            let i = &ds[&data];
            (i.tparams.clone(), i.tvs.clone(), i.name.clone())
        };
        let nparams = tparams.len();
        if args.len() < nparams {
            self.ice(format!("constructor `{name}` without its parameters"));
            let ty = want.cloned().unwrap_or(c::Type::Any);
            return (c::Expr::Panic("internal error".into(), ty.clone()), ty);
        }
        let mut targs = Vec::new();
        for (i, a) in args.iter().take(nparams).enumerate() {
            if tparams[i] {
                let v = self.ev().eval(&cx.env, a);
                targs.push(self.cx_ty(cx, &v));
            }
        }
        let s: Vec<(c::TyVar, c::Type)> = dtvs.iter().cloned().zip(targs.iter().cloned()).collect();
        let ftys: Vec<c::Type> = field_tys.iter().map(|t| t.subst_all(&s)).collect();
        let mut binds = Vec::new();
        let mut lazy: Vec<(c::Binder, c::Expr)> = Vec::new();
        let mut fields = Vec::new();
        let mut fi = 0;
        let given = &args[nparams..];
        // Dictionaries: a method that is not a function may be defined in
        // terms of the dictionary being built, so it becomes a thunk.
        let lazy_from = self.p.traits.get(&data).map(|t| t.supers.len());
        for (j, rt) in runtime.iter().enumerate() {
            if !rt {
                continue;
            }
            if let Some(a) = given.get(j) {
                let (e, _) = self.expr(cx, a, Some(&ftys[fi]));
                if lazy_from.is_some_and(|n| j >= n) && !e.is_atom() && !matches!(a, Term::Lam(..))
                {
                    let id = self.id("m");
                    lazy.push((
                        c::Binder::new(id.clone(), c::Mult::Many, ftys[fi].clone()),
                        e,
                    ));
                    fields.push(c::Expr::Var(id));
                } else {
                    fields.push(self.atom(e, &ftys[fi], &mut binds));
                }
            }
            fi += 1;
        }
        let res_ty = c::Type::Con(dname, targs.clone());
        let missing: Vec<c::Type> = ftys[fields.len()..].to_vec();
        let e = if missing.is_empty() {
            c::Expr::Con(name, targs, fields)
        } else {
            // Partial application: eta-expand.
            let mut ids = Vec::new();
            for t in &missing {
                ids.push((self.id("f"), t.clone()));
            }
            let mut all = fields;
            all.extend(ids.iter().map(|(i, _)| c::Expr::Var(i.clone())));
            let mut e = c::Expr::Con(name, targs, all);
            for (i, t) in ids.into_iter().rev() {
                e = c::Expr::Lam(c::Binder::new(i, c::Mult::Many, t), Box::new(e));
            }
            e
        };
        let mut ty = res_ty;
        for t in missing.into_iter().rev() {
            ty = c::Type::fun(c::Mult::Many, t, ty);
        }
        let (e, ty) = self.coerce(e, &ty, want);
        let e = lazy.into_iter().rev().fold(e, |b, (x, v)| {
            c::Expr::Let(c::Bind::NonRec(x, Box::new(v)), Box::new(b))
        });
        (wrap(binds, e), ty)
    }

    fn case(&self, cx: &mut Cx, k: &Case, want: Option<&c::Type>) -> (c::Expr, c::Type) {
        let res = match want {
            Some(w) => w.clone(),
            None => {
                let rv = self.ev().eval(&cx.env, &k.ret);
                self.cx_ty(cx, &rv)
            }
        };
        let Term::Var(si) = &k.scrut else {
            self.ice("case on a non-variable in lowering".into());
            return (c::Expr::Panic("internal error".into(), res.clone()), res);
        };
        let sl = cx.lvl() - 1 - si;
        if k.alts.is_empty() && k.default.is_none() {
            return (c::Expr::Panic("unreachable".into(), res.clone()), res);
        }
        let sty = self.whnf(&self.ty_val(cx, sl));
        let scrut = cx.vars[sl as usize].clone();
        let Some((sid, scty)) = scrut else {
            // Matching on an erased value: at most one branch.
            return self.erased_case(cx, k, sl, &sty, &res);
        };
        match &sty {
            Val::Data(_, args) => {
                let want_sty = self.cx_ty(cx, &sty);
                let (se, _) = self.coerce(c::Expr::Var(sid), &scty, Some(&want_sty));
                let targs = match &want_sty {
                    c::Type::Con(_, ts) => ts.clone(),
                    _ => Vec::new(),
                };
                let b = c::Binder::new(self.id("s"), c::Mult::Many, want_sty.clone());
                let mut alts = Vec::new();
                for alt in &k.alts {
                    let Alt::Con {
                        con,
                        fields,
                        refine,
                        body,
                    } = alt
                    else {
                        continue;
                    };
                    let mut bcx = cx.clone();
                    let (cname, runtime, ftys, data) = {
                        let cs = self.cons.borrow();
                        let i = &cs[con];
                        (
                            i.name.clone(),
                            i.runtime.clone(),
                            i.field_tys.clone(),
                            i.data,
                        )
                    };
                    let dtvs = self.datas.borrow()[&data].tvs.clone();
                    let s: Vec<(c::TyVar, c::Type)> =
                        dtvs.into_iter().zip(targs.iter().cloned()).collect();
                    let mut binders = Vec::new();
                    let mut fvars = Vec::new();
                    let mut fi = 0;
                    for (j, (x, _, fty)) in fields.iter().enumerate() {
                        let fv = self.ev().eval(&bcx.env, fty);
                        if runtime.get(j).copied().unwrap_or(false) {
                            let t = ftys[fi].subst_all(&s);
                            fi += 1;
                            let id = self.id(x);
                            binders.push(c::Binder::new(id.clone(), c::Mult::Many, t.clone()));
                            let l = self.push(&mut bcx, &fv, None, None, Some((id, t)));
                            fvars.push(Val::var(l));
                        } else {
                            let l = self.push(&mut bcx, &fv, None, None, None);
                            fvars.push(Val::var(l));
                        }
                    }
                    self.refine(&mut bcx, refine, sl, args, *con, fvars);
                    let (be, _) = self.expr(&mut bcx, body, Some(&res));
                    alts.push(c::Alt {
                        con: c::AltCon::Con(cname, binders),
                        rhs: be,
                    });
                }
                if let Some(d) = &k.default {
                    let (de, _) = self.expr(cx, d, Some(&res));
                    alts.push(c::Alt {
                        con: c::AltCon::Default,
                        rhs: de,
                    });
                }
                (c::Expr::Case(Box::new(se), b, res.clone(), alts), res)
            }
            _ => self.lit_case(cx, k, sl, c::Expr::Var(sid), &scty, &res),
        }
    }

    fn refine(
        &self,
        bcx: &mut Cx,
        refine: &[(u32, Term)],
        sl: u32,
        args: &[(Val, Icit)],
        con: GName,
        fvars: Vec<Val>,
    ) {
        for (ix, t) in refine {
            let l = bcx.lvl() - 1 - ix;
            let v = self.ev().eval(&bcx.env, t);
            bcx.env[l as usize] = v;
        }
        let np = match self.p.get(con) {
            Global::Con { nparams, .. } => *nparams,
            _ => 0,
        };
        let mut cargs: Vec<Val> = args.iter().take(np).map(|a| a.0.clone()).collect();
        cargs.extend(fvars);
        bcx.env[sl as usize] = Val::Con(con, cargs);
    }

    fn erased_case(
        &self,
        cx: &mut Cx,
        k: &Case,
        sl: u32,
        sty: &Val,
        res: &c::Type,
    ) -> (c::Expr, c::Type) {
        if let Some(alt) = k.alts.first() {
            match alt {
                Alt::Con {
                    con,
                    fields,
                    refine,
                    body,
                } => {
                    let mut bcx = cx.clone();
                    let mut fvars = Vec::new();
                    for (_, _, fty) in fields {
                        let fv = self.ev().eval(&bcx.env, fty);
                        let l = self.push(&mut bcx, &fv, None, None, None);
                        fvars.push(Val::var(l));
                    }
                    let args = match sty {
                        Val::Data(_, a) => a.clone(),
                        _ => Vec::new(),
                    };
                    self.refine(&mut bcx, refine, sl, &args, *con, fvars);
                    return self.expr(&mut bcx, body, Some(res));
                }
                Alt::Lit { body, .. } => return self.expr(cx, body, Some(res)),
            }
        }
        match &k.default {
            Some(d) => self.expr(cx, d, Some(res)),
            None => (
                c::Expr::Panic("unreachable".into(), res.clone()),
                res.clone(),
            ),
        }
    }

    fn lit_case(
        &self,
        cx: &mut Cx,
        k: &Case,
        sl: u32,
        se: c::Expr,
        scty: &c::Type,
        res: &c::Type,
    ) -> (c::Expr, c::Type) {
        let mut branches = Vec::new();
        for alt in &k.alts {
            if let Alt::Lit { lit, body } = alt {
                let mut bcx = cx.clone();
                bcx.env[sl as usize] = Val::Lit(lit.clone());
                let (be, _) = self.expr(&mut bcx, body, Some(res));
                branches.push((lit.clone(), be));
            }
        }
        let default = match &k.default {
            Some(d) => self.expr(cx, d, Some(res)).0,
            None => c::Expr::Panic("non-exhaustive literal match".into(), res.clone()),
        };
        let first = branches.first().map(|b| b.0.clone());
        let unboxed =
            |con: &str, ty: c::Type, rep: c::Type, k: &dyn Fn(c::Expr) -> c::Expr| -> c::Expr {
                let x = self.id("n");
                let (se2, _) = self.coerce(se.clone(), scty, Some(&ty));
                c::Expr::Case(
                    Box::new(se2),
                    c::Binder::new(self.id("s"), c::Mult::Many, ty),
                    res.clone(),
                    vec![c::Alt {
                        con: c::AltCon::Con(
                            con.into(),
                            vec![c::Binder::new(x.clone(), c::Mult::Many, rep)],
                        ),
                        rhs: k(c::Expr::Var(x)),
                    }],
                )
            };
        let int_switch = |n: c::Expr| -> c::Expr {
            let mut alts: Vec<c::Alt> = branches
                .iter()
                .map(|(l, e)| c::Alt {
                    con: c::AltCon::Int(match l {
                        Lit::Int(v) => *v,
                        Lit::UInt(v) => *v as i64,
                        Lit::Char(ch) => *ch as i64,
                        _ => 0,
                    }),
                    rhs: e.clone(),
                })
                .collect();
            alts.push(c::Alt {
                con: c::AltCon::Default,
                rhs: default.clone(),
            });
            c::Expr::Case(
                Box::new(n),
                c::Binder::new(self.id("k"), c::Mult::Many, c::Type::int()),
                res.clone(),
                alts,
            )
        };
        // An if-chain on an Int# test.
        let chain = |test: &dyn Fn(&Lit) -> c::Expr| -> c::Expr {
            let mut e = default.clone();
            for (l, b) in branches.iter().rev() {
                e = c::Expr::Case(
                    Box::new(test(l)),
                    c::Binder::new(self.id("k"), c::Mult::Many, c::Type::int()),
                    res.clone(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Int(0),
                            rhs: e,
                        },
                        c::Alt {
                            con: c::AltCon::Default,
                            rhs: b.clone(),
                        },
                    ],
                );
            }
            e
        };
        let e = match first {
            None => default.clone(),
            Some(Lit::Int(_)) => unboxed("I#", int_ty(), c::Type::int(), &int_switch),
            Some(Lit::UInt(_)) => unboxed("W#", c::Type::con("UInt"), c::Type::int(), &int_switch),
            Some(Lit::Char(_)) => unboxed("C#", c::Type::con("Char"), c::Type::int(), &int_switch),
            Some(Lit::Float(_)) => unboxed("F#", c::Type::con("Float"), c::Type::double(), &|n| {
                chain(&|l| match l {
                    Lit::Float(x) => prim(
                        c::PrimOp::FEq,
                        vec![n.clone(), c::Expr::Lit(c::Lit::Double(*x))],
                    ),
                    _ => c::Expr::int(0),
                })
            }),
            Some(Lit::Str(_)) => {
                let (s, _) = self.coerce(se.clone(), scty, Some(&c::Type::bytes()));
                chain(&|l| match l {
                    Lit::Str(x) => app(
                        c::Expr::Global("cirka$bytes_eq".into()),
                        vec![s.clone(), c::Expr::Lit(c::Lit::Str(x.as_bytes().to_vec()))],
                    ),
                    _ => c::Expr::int(0),
                })
            }
        };
        (e, res.clone())
    }

    // ---- primitives ----

    fn bool_ty(&self) -> c::Type {
        match self.p.builtins.bool_ {
            Some(b) => c::Type::con(self.p.name(b)),
            None => c::Type::Any,
        }
    }

    fn unit_ty(&self) -> c::Type {
        match self.p.builtins.unit {
            Some(u) => c::Type::con(self.p.name(u)),
            None => c::Type::unit(),
        }
    }

    /// The field of `Borrow# s b` (`s -> Pair s (Ur b)`) or of
    /// `BorrowIO# s b` (`s -> IO (Pair s (Ur b))`).
    fn borrow_field(&self, io: bool, s: c::Type, b: c::Type) -> c::Type {
        let (Some(pair), Some(ur)) = (self.p.builtins.pair, self.p.builtins.mk_ur) else {
            return c::Type::Any;
        };
        let ur_ty = match self.p.get(ur) {
            Global::Con { data, .. } => self.gname(*data),
            _ => String::new(),
        };
        let res = c::Type::Con(
            self.gname(pair),
            vec![s.clone(), c::Type::Con(ur_ty, vec![b])],
        );
        let res = if io {
            c::Type::Con("IO".into(), vec![res])
        } else {
            res
        };
        c::Type::fun(c::Mult::Many, s, res)
    }

    /// The field type of a `Borrow` / `BorrowIO` type.
    fn borrow_field_of(&self, t: &c::Type) -> c::Type {
        match t {
            c::Type::Con(n, a) if a.len() == 2 && (n == "Borrow" || n == "BorrowIO") => {
                self.borrow_field(n == "BorrowIO", a[0].clone(), a[1].clone())
            }
            _ => c::Type::Any,
        }
    }

    /// `λf. K f` for the newtype constructor `K` of the result type.
    fn newtype_wrap(&self, ty: &c::Type, con: &str) -> c::Expr {
        self.skeleton(ty, 1, |a, r| {
            let targs = match &r {
                c::Type::Con(_, t) => t.clone(),
                _ => Vec::new(),
            };
            let field = self.borrow_field_of(&r);
            c::Expr::Con(
                con.into(),
                targs,
                vec![c::Expr::Cast(Box::new(a[0].clone()), field)],
            )
        })
    }

    /// `λm. case m of K f -> f` for the newtype of the parameter.
    fn newtype_run(&self, ty: &c::Type, con: &str) -> c::Expr {
        let mut t = ty;
        while let c::Type::Forall(_, _, b) = t {
            t = b;
        }
        let pty = match t {
            c::Type::Fun(_, a, _) => (**a).clone(),
            _ => c::Type::Any,
        };
        self.skeleton(ty, 1, |a, r| {
            let f = self.id("f");
            c::Expr::Case(
                Box::new(a[0].clone()),
                c::Binder::new(self.id("s"), c::Mult::Many, pty.clone()),
                r.clone(),
                vec![c::Alt {
                    con: c::AltCon::Con(
                        con.into(),
                        vec![c::Binder::new(
                            f.clone(),
                            c::Mult::Many,
                            self.borrow_field_of(&pty),
                        )],
                    ),
                    rhs: c::Expr::Cast(Box::new(c::Expr::Var(f)), r),
                }],
            )
        })
    }

    fn unit_val(&self) -> c::Expr {
        match self.p.builtins.mk_unit {
            Some(u) => c::Expr::Con(self.gname(u), Vec::new(), Vec::new()),
            None => c::Expr::Lit(c::Lit::Unit),
        }
    }

    fn bool_of(&self, e: c::Expr) -> c::Expr {
        let (Some(t), Some(f)) = (self.p.builtins.true_, self.p.builtins.false_) else {
            return c::Expr::Panic("no Bool".into(), c::Type::Any);
        };
        c::Expr::Case(
            Box::new(e),
            c::Binder::new(self.id("b"), c::Mult::Many, c::Type::int()),
            self.bool_ty(),
            vec![
                c::Alt {
                    con: c::AltCon::Int(0),
                    rhs: c::Expr::Con(self.gname(f), Vec::new(), Vec::new()),
                },
                c::Alt {
                    con: c::AltCon::Default,
                    rhs: c::Expr::Con(self.gname(t), Vec::new(), Vec::new()),
                },
            ],
        )
    }

    /// `case e of Con x -> k x`.
    fn unbox(
        &self,
        e: c::Expr,
        con: &str,
        ty: &str,
        rep: c::Type,
        res: c::Type,
        k: impl FnOnce(c::Expr) -> c::Expr,
    ) -> c::Expr {
        let x = self.id("u");
        c::Expr::Case(
            Box::new(e),
            c::Binder::new(self.id("s"), c::Mult::Many, c::Type::con(ty)),
            res,
            vec![c::Alt {
                con: c::AltCon::Con(
                    con.into(),
                    vec![c::Binder::new(x.clone(), c::Mult::Many, rep)],
                ),
                rhs: k(c::Expr::Var(x)),
            }],
        )
    }

    /// Unboxes the `Int`s `es` and passes their `Int#` values to `k`.
    fn unbox_ints(
        &self,
        es: Vec<c::Expr>,
        res: c::Type,
        k: impl FnOnce(Vec<c::Expr>) -> c::Expr,
    ) -> c::Expr {
        fn go<'b>(
            l: &'b Lower<'_>,
            mut es: std::vec::IntoIter<c::Expr>,
            mut done: Vec<c::Expr>,
            res: c::Type,
            k: Box<dyn FnOnce(Vec<c::Expr>) -> c::Expr + 'b>,
        ) -> c::Expr {
            match es.next() {
                None => k(done),
                Some(e) => l.unbox(e, "I#", "Int", c::Type::int(), res.clone(), move |x| {
                    done.push(x);
                    go(l, es, done, res, k)
                }),
            }
        }
        go(self, es.into_iter(), Vec::new(), res, Box::new(k))
    }

    /// `let! _ = e in rest` for `e : Int#`.
    fn seq_int(&self, e: c::Expr, rest: c::Expr) -> c::Expr {
        c::Expr::LetStrict(
            c::Binder::new(self.id("w"), c::Mult::Many, c::Type::int()),
            Box::new(e),
            Box::new(rest),
        )
    }

    fn seq_unit(&self, e: c::Expr, rest: c::Expr) -> c::Expr {
        c::Expr::LetStrict(
            c::Binder::new(self.id("w"), c::Mult::Many, c::Type::unit()),
            Box::new(e),
            Box::new(rest),
        )
    }

    /// Builds `Λ.. λ args. body` following the Core type `ty` for `arity`
    /// value arguments.
    fn skeleton(
        &self,
        ty: &c::Type,
        arity: usize,
        body: impl FnOnce(Vec<c::Expr>, c::Type) -> c::Expr,
    ) -> c::Expr {
        let mut tys = Vec::new();
        let mut t = ty.clone();
        while let c::Type::Forall(tv, k, b) = t {
            tys.push((tv, k));
            t = *b;
        }
        let mut params = Vec::new();
        for _ in 0..arity {
            match t {
                c::Type::Fun(_, a, r) => {
                    params.push((self.id("a"), *a));
                    t = *r;
                }
                other => {
                    t = other;
                    break;
                }
            }
        }
        let mut e = body(
            params
                .iter()
                .map(|(i, _)| c::Expr::Var(i.clone()))
                .collect(),
            t,
        );
        for (i, a) in params.into_iter().rev() {
            e = c::Expr::Lam(c::Binder::new(i, c::Mult::Many, a), Box::new(e));
        }
        for (tv, k) in tys.into_iter().rev() {
            e = c::Expr::TyLam(tv, k, Box::new(e));
        }
        e
    }

    fn prim_body(&self, p: Prim, ty: &c::Type) -> Option<c::Expr> {
        use c::PrimOp as O;
        let int2 = |op: O, con: &'static str, tyn: &'static str| {
            self.skeleton(ty, 2, |a, r| {
                let (x, y) = (a[0].clone(), a[1].clone());
                self.unbox(x, con, tyn, c::Type::int(), r.clone(), |u| {
                    self.unbox(y, con, tyn, c::Type::int(), r.clone(), |v| {
                        c::Expr::Con(con.into(), Vec::new(), vec![prim(op, vec![u, v])])
                    })
                })
            })
        };
        let cmp = |con: &'static str,
                   tyn: &'static str,
                   rep: c::Type,
                   f: &dyn Fn(c::Expr, c::Expr) -> c::Expr| {
            self.skeleton(ty, 2, |a, r| {
                let (x, y) = (a[0].clone(), a[1].clone());
                self.unbox(x, con, tyn, rep.clone(), r.clone(), |u| {
                    self.unbox(y, con, tyn, rep.clone(), r.clone(), |v| {
                        self.bool_of(f(u, v))
                    })
                })
            })
        };
        let flt2 = |op: O| {
            self.skeleton(ty, 2, |a, r| {
                let (x, y) = (a[0].clone(), a[1].clone());
                self.unbox(x, "F#", "Float", c::Type::double(), r.clone(), |u| {
                    self.unbox(y, "F#", "Float", c::Type::double(), r.clone(), |v| {
                        c::Expr::Con("F#".into(), Vec::new(), vec![prim(op, vec![u, v])])
                    })
                })
            })
        };
        let conv = |from: (&'static str, &'static str, c::Type),
                    to: &'static str,
                    f: &dyn Fn(c::Expr) -> c::Expr| {
            self.skeleton(ty, 1, |a, r| {
                self.unbox(a[0].clone(), from.0, from.1, from.2.clone(), r, |u| {
                    c::Expr::Con(to.into(), Vec::new(), vec![f(u)])
                })
            })
        };
        let int_rep = || ("I#", "Int", c::Type::int());
        let id1 = || self.skeleton(ty, 1, |a, _| a[0].clone());
        let g = |n: &str, args: Vec<c::Expr>| app(c::Expr::Global(n.into()), args);
        Some(match p {
            Prim::Int
            | Prim::UInt
            | Prim::Float
            | Prim::Char
            | Prim::String
            | Prim::IO
            | Prim::IO1
            | Prim::Unique
            | Prim::Array
            | Prim::UArray
            | Prim::Region
            | Prim::Ref
            | Prim::Borrow
            | Prim::BorrowIO
            | Prim::Arena
            | Prim::RIO
            | Prim::In
            | Prim::RefIn
            | Prim::Lazy
            | Prim::Subset => return None,
            // `delay f` = `let t = f () in Lazy# t`: the lazy `let` is a
            // thunk, evaluated once by `force`.
            Prim::Delay => self.skeleton(ty, 1, |a, r| {
                let ta = match &r {
                    c::Type::Con(_, ts) if ts.len() == 1 => ts[0].clone(),
                    _ => c::Type::Any,
                };
                let t = self.id("t");
                c::Expr::Let(
                    c::Bind::NonRec(
                        c::Binder::new(t.clone(), c::Mult::Many, ta.clone()),
                        Box::new(c::Expr::App(
                            Box::new(a[0].clone()),
                            Box::new(self.unit_val()),
                        )),
                    ),
                    Box::new(c::Expr::Con(
                        "Lazy#".into(),
                        vec![ta],
                        vec![c::Expr::Var(t)],
                    )),
                )
            }),
            Prim::Force => self.skeleton(ty, 1, |a, r| {
                let (x, y) = (self.id("x"), self.id("y"));
                c::Expr::Case(
                    Box::new(a[0].clone()),
                    c::Binder::new(
                        self.id("s"),
                        c::Mult::Many,
                        c::Type::Con("Lazy".into(), vec![r.clone()]),
                    ),
                    r.clone(),
                    vec![c::Alt {
                        con: c::AltCon::Con(
                            "Lazy#".into(),
                            vec![c::Binder::new(x.clone(), c::Mult::Many, r.clone())],
                        ),
                        rhs: c::Expr::LetStrict(
                            c::Binder::new(y.clone(), c::Mult::Many, r),
                            Box::new(c::Expr::Var(x)),
                            Box::new(c::Expr::Var(y)),
                        ),
                    }],
                )
            }),
            Prim::RegionNew => self.skeleton(ty, 1, |_, _| {
                c::Expr::Extern("cirka_region_new".into(), Vec::new())
            }),
            Prim::RegionFree => self.skeleton(ty, 2, |a, _| {
                self.seq_int(
                    c::Expr::Extern("cirka_region_free".into(), vec![a[0].clone()]),
                    self.unit_val(),
                )
            }),
            Prim::RegionCell => self.skeleton(ty, 3, |a, _| {
                let x = c::Expr::Cast(Box::new(a[1].clone()), c::Type::Any);
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![c::Expr::Extern(
                        "cirka_region_cell".into(),
                        vec![a[0].clone(), x],
                    )],
                )
            }),
            Prim::RegionGet => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r.clone(), |u| {
                    c::Expr::Cast(
                        Box::new(c::Expr::Extern("cirka_region_get".into(), vec![u])),
                        r,
                    )
                })
            }),
            Prim::RegionSet => self.skeleton(ty, 3, |a, r| {
                let x = c::Expr::Cast(Box::new(a[1].clone()), c::Type::Any);
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |u| {
                    self.seq_int(
                        c::Expr::Extern("cirka_region_set".into(), vec![u, x]),
                        self.unit_val(),
                    )
                })
            }),
            // `RIO r a` and `IO a` have the same representation.
            // So do handles and `Int`.
            Prim::RIOWrap
            | Prim::RIORun
            | Prim::InWrap
            | Prim::InCell
            | Prim::RefInWrap
            | Prim::RefInCell => self.skeleton(ty, 1, |a, _| a[0].clone()),
            Prim::RegionVal | Prim::RefVal => self.skeleton(ty, 0, |_, _| self.unit_val()),
            Prim::BorrowWrap => self.newtype_wrap(ty, "Borrow#"),
            Prim::BorrowIOWrap => self.newtype_wrap(ty, "BorrowIO#"),
            Prim::BorrowRun => self.newtype_run(ty, "Borrow#"),
            Prim::BorrowIORun => self.newtype_run(ty, "BorrowIO#"),
            // The value holds only memory: Perceus frees it.
            Prim::DropFree => self.skeleton(ty, 1, |_, _| self.unit_val()),
            Prim::ReleaseFree => self.skeleton(ty, 2, |_, _| self.unit_val()),
            Prim::AddInt => int2(O::Add, "I#", "Int"),
            Prim::SubInt => int2(O::Sub, "I#", "Int"),
            Prim::MulInt => int2(O::Mul, "I#", "Int"),
            Prim::DivInt => int2(O::Quot, "I#", "Int"),
            // Proved safe (spec-v0.0.4.md §5): no check when the proofs are
            // trusted.
            Prim::DivProved if self.p.proofs_trusted => int2(O::QuotNz, "I#", "Int"),
            Prim::DivProved => int2(O::Quot, "I#", "Int"),
            Prim::ModProved if self.p.proofs_trusted => int2(O::RemNz, "I#", "Int"),
            Prim::ModProved => int2(O::Rem, "I#", "Int"),
            Prim::Unreachable => {
                let msg = if self.p.proofs_trusted {
                    cirka_core::expr::UNREACHABLE
                } else {
                    "reached a branch proved unreachable"
                };
                self.skeleton(ty, 1, |_, r| c::Expr::Panic(msg.into(), r))
            }
            Prim::ArrayIndexProved => {
                let op = if self.p.proofs_trusted {
                    O::ArrayGetUnchecked
                } else {
                    O::ArrayGet
                };
                self.skeleton(ty, 2, |a, r| {
                    let elem = r.clone();
                    self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                        c::Expr::Prim(op, vec![elem], vec![a[0].clone(), k])
                    })
                })
            }
            Prim::ModInt => int2(O::Rem, "I#", "Int"),
            Prim::AndInt => int2(O::And, "I#", "Int"),
            Prim::OrInt => int2(O::Or, "I#", "Int"),
            Prim::XorInt => int2(O::Xor, "I#", "Int"),
            Prim::ShlInt => int2(O::Shl, "I#", "Int"),
            Prim::ShrInt => int2(O::Shr, "I#", "Int"),
            Prim::NegInt => conv(int_rep(), "I#", &|u| prim(O::Neg, vec![u])),
            Prim::EqInt => cmp("I#", "Int", c::Type::int(), &|u, v| prim(O::Eq, vec![u, v])),
            Prim::LtInt => cmp("I#", "Int", c::Type::int(), &|u, v| prim(O::Lt, vec![u, v])),
            Prim::LeInt => cmp("I#", "Int", c::Type::int(), &|u, v| prim(O::Le, vec![u, v])),
            Prim::AddUInt => int2(O::Add, "W#", "UInt"),
            Prim::SubUInt => int2(O::Sub, "W#", "UInt"),
            Prim::MulUInt => int2(O::Mul, "W#", "UInt"),
            Prim::EqUInt => cmp("W#", "UInt", c::Type::int(), &|u, v| {
                prim(O::Eq, vec![u, v])
            }),
            Prim::LtUInt => cmp("W#", "UInt", c::Type::int(), &|u, v| {
                g("cirka$ult", vec![u, v])
            }),
            Prim::LeUInt => cmp("W#", "UInt", c::Type::int(), &|u, v| {
                g("cirka$ule", vec![u, v])
            }),
            Prim::IntToUInt => conv(int_rep(), "W#", &|u| u),
            Prim::UIntToInt => conv(("W#", "UInt", c::Type::int()), "I#", &|u| u),
            Prim::AddFloat => flt2(O::FAdd),
            Prim::SubFloat => flt2(O::FSub),
            Prim::MulFloat => flt2(O::FMul),
            Prim::DivFloat => flt2(O::FDiv),
            Prim::NegFloat => conv(("F#", "Float", c::Type::double()), "F#", &|u| {
                prim(O::FNeg, vec![u])
            }),
            Prim::EqFloat => cmp("F#", "Float", c::Type::double(), &|u, v| {
                prim(O::FEq, vec![u, v])
            }),
            Prim::LtFloat => cmp("F#", "Float", c::Type::double(), &|u, v| {
                prim(O::FLt, vec![u, v])
            }),
            Prim::LeFloat => cmp("F#", "Float", c::Type::double(), &|u, v| {
                prim(O::FLe, vec![u, v])
            }),
            Prim::IntToFloat => conv(int_rep(), "F#", &|u| prim(O::IntToDouble, vec![u])),
            Prim::FloatToInt => conv(("F#", "Float", c::Type::double()), "I#", &|u| {
                prim(O::DoubleToInt, vec![u])
            }),
            Prim::SqrtFloat => conv(("F#", "Float", c::Type::double()), "F#", &|u| {
                c::Expr::Extern("sqrt".into(), vec![u])
            }),
            Prim::TruncFloat => conv(("F#", "Float", c::Type::double()), "F#", &|u| {
                c::Expr::Extern("trunc".into(), vec![u])
            }),
            Prim::CharToInt => conv(("C#", "Char", c::Type::int()), "I#", &|u| u),
            Prim::IntToChar => conv(int_rep(), "C#", &|u| u),
            Prim::StrLength => self.skeleton(ty, 1, |a, _| {
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![prim(O::BytesLen, vec![a[0].clone()])],
                )
            }),
            Prim::StrIndex => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |i| {
                    c::Expr::Con(
                        "I#".into(),
                        Vec::new(),
                        vec![prim(O::BytesGet, vec![a[0].clone(), i])],
                    )
                })
            }),
            Prim::StrConcat => self.skeleton(ty, 2, |a, _| g("cirka$bytes_concat", a)),
            Prim::StrEq => self.skeleton(ty, 2, |a, _| self.bool_of(g("cirka$bytes_eq", a))),
            // `primStrSlice s i j`: a copy of bytes `[i, j)` (clamped).
            Prim::StrSlice => self.skeleton(ty, 3, |a, r| {
                let s = a[0].clone();
                self.unbox_ints(a[1..].to_vec(), r, |k| {
                    g("cirka$bytes_slice", vec![s, k[0].clone(), k[1].clone()])
                })
            }),
            // `primStrFindByte s b i j`: the first index in `[i, j)` of
            // byte `b`, or -1.
            Prim::StrFindByte => self.skeleton(ty, 4, |a, r| {
                let s = a[0].clone();
                self.unbox_ints(a[1..].to_vec(), r, |k| {
                    let mut args = vec![s];
                    args.extend(k);
                    int_con(g("cirka$bytes_find_byte", args))
                })
            }),
            // `primStrFind s i j p`: the first index in `[i, j)` where `p`
            // starts, or -1.
            Prim::StrFind => self.skeleton(ty, 4, |a, r| {
                let (s, p) = (a[0].clone(), a[3].clone());
                self.unbox_ints(a[1..3].to_vec(), r, |k| {
                    int_con(g(
                        "cirka$bytes_find",
                        vec![s, k[0].clone(), k[1].clone(), p],
                    ))
                })
            }),
            // `primStrRangeEq s i t j n`: bytes `s[i, i+n)` and `t[j, j+n)`
            // are equal (False when out of range).
            Prim::StrRangeEq => self.skeleton(ty, 5, |a, r| {
                let (s, t) = (a[0].clone(), a[2].clone());
                self.unbox_ints(vec![a[1].clone(), a[3].clone(), a[4].clone()], r, |k| {
                    self.bool_of(g(
                        "cirka$bytes_range_eq",
                        vec![s, k[0].clone(), t, k[1].clone(), k[2].clone()],
                    ))
                })
            }),
            Prim::StrNew => self.skeleton(ty, 1, |a, r| {
                self.unbox_ints(vec![a[0].clone()], r, |k| {
                    prim(O::BytesNew, vec![k[0].clone()])
                })
            }),
            // `primStrWrite u off s i j`: copies `s[i, j)` to `u` at `off`
            // in place (the string is copied first when shared).
            Prim::StrWrite => self.skeleton(ty, 5, |a, r| {
                let (u, s) = (a[0].clone(), a[2].clone());
                self.unbox_ints(vec![a[1].clone(), a[3].clone(), a[4].clone()], r, |k| {
                    g(
                        "cirka$bytes_write",
                        vec![
                            prim(O::BytesUnique, vec![u]),
                            k[0].clone(),
                            s,
                            k[1].clone(),
                            k[2].clone(),
                        ],
                    )
                })
            }),
            Prim::StrSetByte => self.skeleton(ty, 3, |a, r| {
                let u = a[0].clone();
                self.unbox_ints(a[1..].to_vec(), r, |k| {
                    let b = self.id("b");
                    c::Expr::LetStrict(
                        c::Binder::new(b.clone(), c::Mult::Many, c::Type::bytes()),
                        Box::new(prim(O::BytesUnique, vec![u])),
                        Box::new(self.seq_unit(
                            prim(
                                O::BytesSet,
                                vec![c::Expr::Var(b.clone()), k[0].clone(), k[1].clone()],
                            ),
                            c::Expr::Var(b),
                        )),
                    )
                })
            }),
            Prim::IntToStr => self.skeleton(ty, 1, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |u| {
                    g("cirka$int_to_bytes", vec![u])
                })
            }),
            Prim::FloatToStr => self.skeleton(ty, 1, |a, r| {
                self.unbox(a[0].clone(), "F#", "Float", c::Type::double(), r, |u| {
                    c::Expr::Extern("cirka_f64_to_bytes".into(), vec![u])
                })
            }),
            Prim::CharToStr => self.skeleton(ty, 1, |a, r| {
                self.unbox(a[0].clone(), "C#", "Char", c::Type::int(), r, |u| {
                    g("cirka$char_to_bytes", vec![u])
                })
            }),
            Prim::Pure => self.skeleton(ty, 2, |a, _| a[0].clone()),
            Prim::Bind => self.skeleton(ty, 3, |a, r| {
                let (m, f, w) = (a[0].clone(), a[1].clone(), a[2].clone());
                let x = self.id("x");
                let xt = match ty {
                    c::Type::Forall(tv, _, _) => c::Type::Var(tv.clone()),
                    _ => c::Type::Any,
                };
                let _ = r;
                c::Expr::LetStrict(
                    c::Binder::new(x.clone(), c::Mult::Many, xt),
                    Box::new(app(m, vec![w.clone()])),
                    Box::new(app(f, vec![c::Expr::Var(x), w])),
                )
            }),
            Prim::PutStr => self.skeleton(ty, 2, |a, _| {
                self.seq_unit(
                    c::Expr::Extern("cirka_write_bytes".into(), vec![a[0].clone()]),
                    self.unit_val(),
                )
            }),
            Prim::PrintInt => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |u| {
                    self.seq_unit(
                        c::Expr::Extern("cirka_write_i64".into(), vec![u]),
                        self.seq_unit(
                            c::Expr::Extern("cirka_write_char".into(), vec![c::Expr::int(10)]),
                            self.unit_val(),
                        ),
                    )
                })
            }),
            Prim::PrintFloat => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "F#", "Float", c::Type::double(), r, |u| {
                    self.seq_unit(
                        c::Expr::Extern("cirka_write_f64".into(), vec![u]),
                        self.seq_unit(
                            c::Expr::Extern("cirka_write_char".into(), vec![c::Expr::int(10)]),
                            self.unit_val(),
                        ),
                    )
                })
            }),
            Prim::PutChar => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "C#", "Char", c::Type::int(), r, |u| {
                    self.seq_unit(
                        c::Expr::Extern("cirka_write_char".into(), vec![u]),
                        self.unit_val(),
                    )
                })
            }),
            Prim::ReadInt => self.skeleton(ty, 1, |a, _| {
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![g("cirka$read_int", vec![a[0].clone()])],
                )
            }),
            Prim::ArgCount => self.skeleton(ty, 1, |_, _| {
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![c::Expr::Extern("cirka_argc".into(), Vec::new())],
                )
            }),
            Prim::GetArg => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |u| {
                    c::Expr::Extern("cirka_arg".into(), vec![u])
                })
            }),
            Prim::Exit => self.skeleton(ty, 2, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r.clone(), |u| {
                    self.seq_unit(
                        c::Expr::Extern("cirka_exit".into(), vec![u]),
                        c::Expr::Panic("unreachable".into(), r),
                    )
                })
            }),
            Prim::Panic => self.skeleton(ty, 1, |a, r| {
                self.seq_unit(
                    c::Expr::Extern("cirka_panic_bytes".into(), vec![a[0].clone()]),
                    c::Expr::Panic("unreachable".into(), r),
                )
            }),
            Prim::MkUnique | Prim::Freeze => id1(),
            // Proofs are erased: rewriting is the identity (spec-v0.0.4.md §2.2).
            Prim::Rewrite | Prim::Refine => id1(),
            // ---- arrays (spec-v0.0.2.md §7) ----
            Prim::ArrayNew => self.skeleton(ty, 2, |a, r| {
                let elem = array_elem(&r);
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |k| {
                    c::Expr::Prim(O::ArrayNew, vec![elem], vec![k, a[1].clone()])
                })
            }),
            // No element is ever read: the placeholder is never seen.
            Prim::ArrayEmpty => self.skeleton(ty, 1, |_, r| {
                let elem = array_elem(&r);
                c::Expr::Prim(
                    O::ArrayNew,
                    vec![elem.clone()],
                    vec![
                        c::Expr::int(0),
                        c::Expr::Cast(Box::new(self.unit_val()), elem),
                    ],
                )
            }),
            Prim::ArraySize => self.skeleton(ty, 1, |a, _| {
                let elem = self.array_elem_of(ty, 0);
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![c::Expr::Prim(O::ArrayLen, vec![elem], vec![a[0].clone()])],
                )
            }),
            Prim::ArrayIndex => self.skeleton(ty, 2, |a, r| {
                let elem = r.clone();
                self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                    c::Expr::Prim(O::ArrayGet, vec![elem], vec![a[0].clone(), k])
                })
            }),
            Prim::ArrayRead => self.skeleton(ty, 2, |a, r| {
                let elem = self.array_elem_of(ty, 0);
                let arr = a[0].clone();
                let x = self.id("x");
                let rr = r.clone();
                self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                    c::Expr::LetStrict(
                        c::Binder::new(x.clone(), c::Mult::Many, elem.clone()),
                        Box::new(c::Expr::Prim(O::ArrayGet, vec![elem], vec![arr.clone(), k])),
                        Box::new(self.pair_ur(&rr, arr, c::Expr::Var(x))),
                    )
                })
            }),
            Prim::ArrayLength => self.skeleton(ty, 1, |a, r| {
                let elem = self.array_elem_of(ty, 0);
                let k = self.id("k");
                c::Expr::LetStrict(
                    c::Binder::new(k.clone(), c::Mult::Many, c::Type::int()),
                    Box::new(c::Expr::Prim(O::ArrayLen, vec![elem], vec![a[0].clone()])),
                    Box::new(self.pair_val(
                        &r,
                        a[0].clone(),
                        c::Expr::Con("I#".into(), Vec::new(), vec![c::Expr::Var(k)]),
                    )),
                )
            }),
            Prim::ArrayWrite => self.skeleton(ty, 3, |a, r| {
                let elem = array_elem(&r);
                let b = self.id("b");
                let arr_ty = r.clone();
                self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                    c::Expr::LetStrict(
                        c::Binder::new(b.clone(), c::Mult::Many, arr_ty),
                        Box::new(c::Expr::Prim(
                            O::ArrayUnique,
                            vec![elem.clone()],
                            vec![a[0].clone()],
                        )),
                        Box::new(self.seq_unit(
                            c::Expr::Prim(
                                O::ArraySet,
                                vec![elem],
                                vec![c::Expr::Var(b.clone()), k, a[2].clone()],
                            ),
                            c::Expr::Var(b),
                        )),
                    )
                })
            }),
            Prim::ArrayThaw => self.skeleton(ty, 1, |a, r| {
                let elem = array_elem(&r);
                c::Expr::Prim(O::ArrayUnique, vec![elem], vec![a[0].clone()])
            }),
            Prim::UArrayNew => self.skeleton(ty, 1, |a, r| {
                self.unbox(a[0].clone(), "I#", "Int", c::Type::int(), r, |k| {
                    g("cirka$uarr_new", vec![k])
                })
            }),
            Prim::UArraySize => self.skeleton(ty, 1, |a, _| {
                c::Expr::Con(
                    "I#".into(),
                    Vec::new(),
                    vec![prim(
                        O::Shr,
                        vec![prim(O::BytesLen, vec![a[0].clone()]), c::Expr::int(3)],
                    )],
                )
            }),
            Prim::UArrayLength => self.skeleton(ty, 1, |a, r| {
                let k = self.id("k");
                c::Expr::LetStrict(
                    c::Binder::new(k.clone(), c::Mult::Many, c::Type::int()),
                    Box::new(prim(
                        O::Shr,
                        vec![prim(O::BytesLen, vec![a[0].clone()]), c::Expr::int(3)],
                    )),
                    Box::new(self.pair_val(
                        &r,
                        a[0].clone(),
                        c::Expr::Con("I#".into(), Vec::new(), vec![c::Expr::Var(k)]),
                    )),
                )
            }),
            Prim::UArrayThaw => {
                self.skeleton(ty, 1, |a, _| prim(O::BytesUnique, vec![a[0].clone()]))
            }
            Prim::UArrayIndexWord => self.uarr_index(ty, O::BytesGet64, "I#", c::Type::int()),
            Prim::UArrayIndexFloat => self.uarr_index(ty, O::BytesGetF, "F#", c::Type::double()),
            Prim::UArrayReadWord => self.uarr_read(ty, O::BytesGet64, "I#", c::Type::int()),
            Prim::UArrayReadFloat => self.uarr_read(ty, O::BytesGetF, "F#", c::Type::double()),
            Prim::UArrayWriteWord => {
                self.uarr_write(ty, O::BytesSet64, ("I#", "Int"), c::Type::int())
            }
            Prim::UArrayWriteFloat => {
                self.uarr_write(ty, O::BytesSetF, ("F#", "Float"), c::Type::double())
            }
        })
    }

    /// The element type of the `n`-th parameter (`Array a`) of the Core
    /// type `ty` of a primitive.
    fn array_elem_of(&self, ty: &c::Type, n: usize) -> c::Type {
        let mut t = ty;
        while let c::Type::Forall(_, _, b) = t {
            t = b;
        }
        let mut k = 0;
        while let c::Type::Fun(_, a, r) = t {
            if k == n {
                return array_elem(a);
            }
            k += 1;
            t = r;
        }
        c::Type::Any
    }

    /// `MkPair a b` at the Core type `ty` (`Pair A B`).
    fn pair_val(&self, ty: &c::Type, a: c::Expr, b: c::Expr) -> c::Expr {
        let Some(mk) = self.p.builtins.mk_pair else {
            return c::Expr::Panic("no Pair".into(), ty.clone());
        };
        let targs = match ty {
            c::Type::Con(_, args) => args.clone(),
            _ => Vec::new(),
        };
        c::Expr::Con(self.gname(mk), targs, vec![a, b])
    }

    /// `MkPair a (MkUr b)` at the Core type `ty` (`Pair A (Ur B)`).
    fn pair_ur(&self, ty: &c::Type, a: c::Expr, b: c::Expr) -> c::Expr {
        let Some(mk_ur) = self.p.builtins.mk_ur else {
            return c::Expr::Panic("no Ur".into(), ty.clone());
        };
        let ur_args = match ty {
            c::Type::Con(_, args) => match args.get(1) {
                Some(c::Type::Con(_, u)) => u.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        self.pair_val(ty, a, c::Expr::Con(self.gname(mk_ur), ur_args, vec![b]))
    }

    /// `primUArrayIndex*`: the word at index `i`, boxed with `con`.
    fn uarr_index(&self, ty: &c::Type, op: c::PrimOp, con: &str, rep: c::Type) -> c::Expr {
        let _ = rep;
        self.skeleton(ty, 2, |a, r| {
            self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                c::Expr::Con(
                    con.into(),
                    Vec::new(),
                    vec![c::Expr::Prim(op, Vec::new(), vec![a[0].clone(), k])],
                )
            })
        })
    }

    /// `primUArrayRead*`: the word at index `i` and the array back.
    fn uarr_read(&self, ty: &c::Type, op: c::PrimOp, con: &str, rep: c::Type) -> c::Expr {
        self.skeleton(ty, 2, |a, r| {
            let x = self.id("x");
            let rr = r.clone();
            let arr = a[0].clone();
            self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r, |k| {
                c::Expr::LetStrict(
                    c::Binder::new(x.clone(), c::Mult::Many, rep),
                    Box::new(c::Expr::Prim(op, Vec::new(), vec![arr.clone(), k])),
                    Box::new(self.pair_ur(
                        &rr,
                        arr,
                        c::Expr::Con(con.into(), Vec::new(), vec![c::Expr::Var(x)]),
                    )),
                )
            })
        })
    }

    /// `primUArrayWrite*`: stores the word in place (copying the array
    /// first when it is shared) and returns the array.
    fn uarr_write(
        &self,
        ty: &c::Type,
        op: c::PrimOp,
        (con, tyn): (&str, &str),
        rep: c::Type,
    ) -> c::Expr {
        self.skeleton(ty, 3, |a, r| {
            let b = self.id("b");
            let (arr, v) = (a[0].clone(), a[2].clone());
            self.unbox(a[1].clone(), "I#", "Int", c::Type::int(), r.clone(), |k| {
                self.unbox(v, con, tyn, rep, r, |x| {
                    c::Expr::LetStrict(
                        c::Binder::new(b.clone(), c::Mult::Many, c::Type::bytes()),
                        Box::new(prim(c::PrimOp::BytesUnique, vec![arr])),
                        Box::new(self.seq_unit(
                            c::Expr::Prim(op, Vec::new(), vec![c::Expr::Var(b.clone()), k, x]),
                            c::Expr::Var(b),
                        )),
                    )
                })
            })
        })
    }

    // ---- program ----

    fn program(&self) -> Result<c::Program, Vec<Diagnostic>> {
        let mut prog = match c::parse_program(cirka_diagnostics::FileId(u32::MAX), SUPPORT) {
            Ok(p) => p,
            Err(e) => {
                return Err(vec![Diagnostic::ice(format!(
                    "support library: {}",
                    e.message
                ))]);
            }
        };
        for (n, con, rep) in [
            ("Int", "I#", c::Type::int()),
            ("UInt", "W#", c::Type::int()),
            ("Float", "F#", c::Type::double()),
            ("Char", "C#", c::Type::int()),
        ] {
            prog.datas.push(c::DataDecl {
                name: n.into(),
                params: Vec::new(),
                linear: false,
                ctors: vec![c::CtorDecl {
                    name: con.into(),
                    fields: vec![rep],
                    strict: vec![true],
                }],
                newtype: false,
            });
        }
        let io_tv = self.tv("a");
        prog.datas.push(c::DataDecl {
            name: "IO".into(),
            params: vec![(io_tv.clone(), c::Kind::Type)],
            linear: false,
            ctors: vec![c::CtorDecl {
                name: "IO#".into(),
                fields: vec![c::Type::fun(
                    c::Mult::Many,
                    c::Type::unit(),
                    c::Type::Var(io_tv),
                )],
                strict: vec![true],
            }],
            newtype: true,
        });
        // Lazy a = Lazy# a with a lazy field (spec-v0.0.3.md §4).
        {
            let av = self.tv("a");
            prog.datas.push(c::DataDecl {
                name: "Lazy".into(),
                params: vec![(av.clone(), c::Kind::Type)],
                linear: false,
                ctors: vec![c::CtorDecl {
                    name: "Lazy#".into(),
                    fields: vec![c::Type::Var(av)],
                    strict: vec![false],
                }],
                newtype: false,
            });
        }
        // Borrow s b = Borrow# (s -> Pair s (Ur b)) and BorrowIO s b =
        // BorrowIO# (s -> IO (Pair s (Ur b))) (spec-v0.0.3.md §2).
        for (name, con, io) in [
            ("Borrow", "Borrow#", false),
            ("BorrowIO", "BorrowIO#", true),
        ] {
            let (sv, bv) = (self.tv("s"), self.tv("b"));
            let field = self.borrow_field(io, c::Type::Var(sv.clone()), c::Type::Var(bv.clone()));
            prog.datas.push(c::DataDecl {
                name: name.into(),
                params: vec![(sv, c::Kind::Type), (bv, c::Kind::Type)],
                linear: false,
                ctors: vec![c::CtorDecl {
                    name: con.into(),
                    fields: vec![field],
                    strict: vec![true],
                }],
                newtype: true,
            });
        }
        let datas: Vec<GName> = self
            .p
            .order
            .iter()
            .copied()
            .filter(|g| matches!(self.p.get(*g), Global::Data { .. }))
            .collect();
        let mut decls = Vec::new();
        for d in &datas {
            self.data(*d, &mut decls);
        }
        for (d, decl) in datas.iter().zip(decls.iter_mut()) {
            self.data_ctors(*d, decl);
        }
        prog.datas.extend(decls);
        for g in &self.p.order {
            match self.p.get(*g) {
                Global::Def { ty, body: Some(b) } => {
                    let tv = self.ev().eval(&Vec::new(), ty);
                    if self.is_universe_valued(&tv) {
                        continue;
                    }
                    self.fuel.set(FUEL);
                    let cty = self.lower_ty(&mut Vec::new(), &tv);
                    let (e, _) = self.expr(&mut Cx::default(), b, Some(&cty));
                    prog.binds.push(c::TopBind {
                        name: self.gname(*g),
                        ty: cty,
                        expr: e,
                        span: Span::DUMMY,
                    });
                }
                Global::Prim { .. } | Global::Extern { .. } => {
                    let tv = self.ev().eval(&Vec::new(), self.p.type_of(*g).unwrap());
                    let cty = self.lower_ty(&mut Vec::new(), &tv);
                    // Primitives are written against the representation of `IO`.
                    let rep = io_rep(&cty);
                    let body = match self.p.get(*g) {
                        Global::Prim { prim, .. } => self.prim_body(*prim, &rep),
                        Global::Extern { ty, abi } => match self.extern_fn(*g, ty, *abi) {
                            Some((e, _, d)) => {
                                match prog.externs.iter().find(|x| x.name == d.name) {
                                    Some(x) if !externs::same_call(x, &d) => {
                                        self.user_errs.borrow_mut().push(
                                            Diagnostic::error(format!(
                                                "`{}` is declared with two different signatures",
                                                d.name
                                            ))
                                            .with_code("E1271")
                                            .with_note(
                                                "every `extern` declaration of a function \
                                                 (and the compiler's own use of it) must \
                                                 pass the same values",
                                            ),
                                        )
                                    }
                                    Some(_) => {}
                                    None => prog.externs.push(d),
                                }
                                Some(e)
                            }
                            None => None,
                        },
                        _ => None,
                    };
                    if let Some(e) = body {
                        let name = self.gname(*g);
                        if rep.alpha_eq(&cty) {
                            prog.binds.push(c::TopBind {
                                name,
                                ty: cty,
                                expr: e,
                                span: Span::DUMMY,
                            });
                        } else {
                            let target = format!("{name}$rep");
                            prog.binds.push(c::TopBind {
                                name: target.clone(),
                                ty: rep,
                                expr: e,
                                span: Span::DUMMY,
                            });
                            prog.binds.push(c::TopBind {
                                name,
                                expr: cast_poly(c::Expr::Global(target), &cty),
                                ty: cty,
                                span: Span::DUMMY,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        if let (Some(m), Some(unit)) = (self.p.main, self.p.builtins.unit) {
            let r = self.id("r");
            prog.binds.push(c::TopBind {
                name: "main".into(),
                ty: c::Type::int(),
                expr: c::Expr::LetStrict(
                    c::Binder::new(r, c::Mult::Many, c::Type::con(self.p.name(unit))),
                    Box::new(app(
                        c::Expr::Cast(
                            Box::new(c::Expr::Global(self.gname(m))),
                            c::Type::fun(
                                c::Mult::Many,
                                c::Type::unit(),
                                c::Type::con(self.p.name(unit)),
                            ),
                        ),
                        vec![c::Expr::Lit(c::Lit::Unit)],
                    )),
                    Box::new(c::Expr::int(0)),
                ),
                span: Span::DUMMY,
            });
        }
        let user = self.user_errs.take();
        if !user.is_empty() {
            return Err(user);
        }
        let errs = self.errs.take();
        if !errs.is_empty() {
            return Err(errs
                .into_iter()
                .map(|m| Diagnostic::ice(format!("lowering: {m}")))
                .collect());
        }
        Ok(prog)
    }
}
