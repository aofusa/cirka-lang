//! Core → MIR (06-ir.md §3). The result contains no dup/drop; run
//! [`crate::perceus::run`] on it afterwards.
//!
//! Evaluation (lazy Core semantics, strict `let!` / `case`):
//! - Function results, thunk results, `case` binders, `let!` variables and
//!   constructor fields marked strict are in WHNF. Function arguments,
//!   lazy fields and lazily `let`-bound variables may be thunks and are
//!   `force`d where their value is needed.
//! - A non-atomic argument or lazy field becomes a thunk (`mkthunk` of a
//!   lifted function over its free variables). Unboxed values are always
//!   evaluated eagerly.
//! - Lambdas used as values are lifted to closure-convention functions
//!   (all parameters and the result are pointers; unboxed values are boxed
//!   in `$BoxI` / `$BoxF`). `letrec`-bound lambdas are lambda-lifted and
//!   called directly when saturated. Top-level functions keep native
//!   representations and get a `$clo` wrapper when used as values.
//! - Top-level non-functions become CAFs (`global`), except unboxed ones,
//!   which are recomputed by a parameterless function.

use crate::ir::{
    self, Block, BlockId, Const, CtorRef, Function, Instr, Module, Operand, Own, Rhs, Terminator,
    Ty, Var,
};
use cirka_core::{self as core, AltCon, Bind, Expr, Id, Lit, PrimOp, Program, Type};
use cirka_diagnostics::Diagnostic;
use std::collections::{HashMap, HashSet};

/// Lowers a (linted) Core program to MIR without reference counting.
pub fn lower_program(p: &Program) -> Result<Module, Vec<Diagnostic>> {
    let mut l = Lowerer::new(p);
    l.run();
    l.m.collect_cycles = p.collect_cycles;
    if l.errors.is_empty() {
        Ok(l.m)
    } else {
        Err(l.errors)
    }
}

#[derive(Clone, Debug)]
enum Bound {
    /// A value (maybe a thunk when `whnf` is false).
    Val { op: Operand, ty: Type, whnf: bool },
    /// A lambda-lifted local function: call `func(fvs..., args...)`.
    Known {
        func: String,
        fvs: Vec<Id>,
        arity: usize,
        ty: Type,
    },
}

impl Bound {
    fn ty(&self) -> &Type {
        match self {
            Bound::Val { ty, .. } | Bound::Known { ty, .. } => ty,
        }
    }
}

type Env = HashMap<u32, Bound>;

#[derive(Clone, Debug)]
enum Top {
    Fun { mir: String, arity: usize },
    Caf { global: String },
    Const { mir: String },
}

/// Signature of a generated MIR function.
#[derive(Clone, Debug)]
struct Sig {
    params: Vec<Ty>,
    ret: Ty,
}

struct DataInfo {
    newtype: bool,
}

struct Lowerer<'p> {
    p: &'p Program,
    m: Module,
    tops: HashMap<String, Top>,
    sigs: HashMap<String, Sig>,
    ctors: HashMap<String, (CtorRef, Vec<bool>)>,
    datas: HashMap<String, DataInfo>,
    box_i: CtorRef,
    box_f: CtorRef,
    wrappers: HashMap<String, String>,
    /// Functions returning several values (an unboxed tuple in Core,
    /// 06-ir.md §1.7) and their result types.
    multi: HashMap<String, Vec<Ty>>,
    counter: u32,
    errors: Vec<Diagnostic>,
    /// Functions still to be lowered: (name, env, binders, body, closure convention).
    pending: Vec<Pending>,
}

struct Pending {
    name: String,
    env: Env,
    /// Leading captured variables (Core ids) with their MIR types and
    /// whether they were evaluated where they were captured.
    fvs: Vec<(Id, Type, bool)>,
    params: Vec<core::Binder>,
    body: Expr,
    /// Closure convention: pointer parameters and result.
    closure: bool,
    /// Parameters every call passes evaluated (FEAT-0066): not forced.
    whnf: Vec<bool>,
}

/// Builder for one MIR function.
struct Fb {
    f: Function,
    cur: usize,
    nlabels: u32,
}

impl Fb {
    fn new(name: &str, ret: Ty) -> Fb {
        let mut f = Function::new(name, ret);
        f.blocks.push(Block {
            label: "entry".into(),
            params: vec![],
            instrs: vec![],
            term: Terminator::Unreachable,
        });
        Fb {
            f,
            cur: 0,
            nlabels: 0,
        }
    }

    fn block(&mut self, hint: &str) -> BlockId {
        self.nlabels += 1;
        self.f.blocks.push(Block {
            label: format!("{hint}{}", self.nlabels),
            params: vec![],
            instrs: vec![],
            term: Terminator::Unreachable,
        });
        BlockId(self.f.blocks.len() as u32 - 1)
    }

    fn switch_to(&mut self, b: BlockId) {
        self.cur = b.0 as usize;
    }

    fn emit(&mut self, i: Instr) {
        self.f.blocks[self.cur].instrs.push(i);
    }

    fn let_(&mut self, hint: &str, ty: Ty, rhs: Rhs) -> Var {
        let v = self.f.fresh_var(hint, ty);
        self.emit(Instr::Let { var: Some(v), rhs });
        v
    }

    fn term(&mut self, t: Terminator) {
        self.f.blocks[self.cur].term = t;
    }
}

fn strip_tylams(mut e: &Expr) -> &Expr {
    while let Expr::TyLam(_, _, b) = e {
        e = b;
    }
    e
}

/// Lambda binders of a lambda chain (type abstractions are skipped).
fn lam_chain(e: &Expr) -> (Vec<core::Binder>, &Expr) {
    let mut bs = Vec::new();
    let mut e = strip_tylams(e);
    while let Expr::Lam(b, body) = e {
        bs.push(b.clone());
        e = strip_tylams(body);
    }
    (bs, e)
}

/// Head and value arguments of an application (type applications and
/// casts on the head are skipped).
fn spine(e: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut args = Vec::new();
    let mut e = e;
    loop {
        match e {
            Expr::App(f, a) => {
                args.push(&**a);
                e = f;
            }
            Expr::TyApp(f, _) => e = f,
            _ => break,
        }
    }
    args.reverse();
    (e, args)
}

fn strip_fun(t: &Type) -> &Type {
    match t {
        Type::Forall(_, _, b) => strip_fun(b),
        t => t,
    }
}

impl<'p> Lowerer<'p> {
    fn new(p: &'p Program) -> Lowerer<'p> {
        let mut m = Module::default();
        let mut ctors = HashMap::new();
        let mut datas = HashMap::new();
        let mut l = Lowerer {
            p,
            m: Module::default(),
            tops: HashMap::new(),
            sigs: HashMap::new(),
            ctors: HashMap::new(),
            datas: HashMap::new(),
            box_i: CtorRef { ty: 0, ctor: 0 },
            box_f: CtorRef { ty: 0, ctor: 0 },
            wrappers: HashMap::new(),
            multi: HashMap::new(),
            counter: 0,
            errors: Vec::new(),
            pending: Vec::new(),
        };
        for d in &p.datas {
            datas.insert(d.name.clone(), DataInfo { newtype: d.newtype });
        }
        l.datas = datas;
        for d in &p.datas {
            if d.newtype {
                continue;
            }
            let ti = m.types.len() as u32;
            let mut cs = Vec::new();
            for (ci, c) in d.ctors.iter().enumerate() {
                let fields = c
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| ir::Field {
                        name: format!("f{i}"),
                        ty: l.rep(f),
                    })
                    .collect();
                cs.push(ir::CtorDef {
                    name: c.name.clone(),
                    tag: ci as u32,
                    fields,
                });
                ctors.insert(
                    c.name.clone(),
                    (
                        CtorRef {
                            ty: ti,
                            ctor: ci as u32,
                        },
                        c.strict.clone(),
                    ),
                );
            }
            m.types.push(ir::TypeDef {
                name: d.name.clone(),
                ctors: cs,
            });
        }
        for (name, ty) in [("$BoxI", Ty::I64), ("$BoxF", Ty::F64)] {
            let ti = m.types.len() as u32;
            m.types.push(ir::TypeDef {
                name: name.into(),
                ctors: vec![ir::CtorDef {
                    name: "B".into(),
                    tag: 0,
                    fields: vec![ir::Field {
                        name: "v".into(),
                        ty,
                    }],
                }],
            });
            let r = CtorRef { ty: ti, ctor: 0 };
            if ty == Ty::I64 {
                l.box_i = r;
            } else {
                l.box_f = r;
            }
        }
        for e in &p.externs {
            m.externs.push(ir::ExternDecl {
                name: e.name.clone(),
                params: e
                    .params
                    .iter()
                    .map(|t| {
                        let r = l.rep(t);
                        (
                            r,
                            if r.is_ptr() {
                                Own::Borrowed
                            } else {
                                Own::Owned
                            },
                        )
                    })
                    .collect(),
                ret: if e.ret.alpha_eq(&Type::unit()) {
                    None
                } else {
                    Some(l.rep(&e.ret))
                },
                c_sig: e.c_sig.clone(),
            });
        }
        l.m = m;
        l.ctors = ctors;
        l
    }

    /// MIR representation of a Core type (newtypes are transparent).
    fn rep(&self, t: &Type) -> Ty {
        self.rep_depth(t, 0)
    }

    fn rep_depth(&self, t: &Type, depth: u32) -> Ty {
        if let Some(r) = t.rep() {
            return match r {
                core::Rep::I64 => Ty::I64,
                core::Rep::F64 => Ty::F64,
            };
        }
        if let Type::Con(c, args) = t
            && depth < 32
            && let Some(d) = self.p.data(c)
            && d.newtype
            && args.len() == d.params.len()
        {
            let s: Vec<_> = d
                .params
                .iter()
                .map(|p| p.0.clone())
                .zip(args.iter().cloned())
                .collect();
            return self.rep_depth(&d.ctors[0].fields[0].subst_all(&s), depth + 1);
        }
        Ty::Ptr
    }

    fn fresh_name(&mut self, base: &str, kind: &str) -> String {
        self.counter += 1;
        format!("{base}${kind}{}", self.counter)
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.errors
            .push(Diagnostic::error(msg.into()).with_code("C0200"));
    }

    fn run(&mut self) {
        let p = self.p;
        // Classify top-level bindings.
        for b in &p.binds {
            let (params, _) = lam_chain(&b.expr);
            let top = if !params.is_empty() {
                let params_ty: Vec<Ty> = params.iter().map(|x| self.rep(&x.ty)).collect();
                let rt = self.result_type(&b.ty, params.len());
                if let Type::Con(c, ts) = &rt
                    && core::cpr::is_utup(c)
                {
                    let rs: Vec<Ty> = ts.iter().map(|t| self.rep(t)).collect();
                    self.multi.insert(b.name.clone(), rs);
                }
                // A function with several results: its first one (as in
                // `Function::ret`).
                let ret = match self.multi.get(&b.name) {
                    Some(rs) => rs[0],
                    None => self.rep(&rt),
                };
                self.sigs.insert(
                    b.name.clone(),
                    Sig {
                        params: params_ty,
                        ret,
                    },
                );
                Top::Fun {
                    mir: b.name.clone(),
                    arity: params.len(),
                }
            } else if self.rep(&b.ty) != Ty::Ptr || b.name == "main" {
                self.sigs.insert(
                    b.name.clone(),
                    Sig {
                        params: vec![],
                        ret: self.rep(&b.ty),
                    },
                );
                Top::Const {
                    mir: b.name.clone(),
                }
            } else {
                let init = format!("{}$init", b.name);
                self.sigs.insert(
                    init.clone(),
                    Sig {
                        params: vec![],
                        ret: Ty::Ptr,
                    },
                );
                self.m.globals.push(ir::GlobalDecl {
                    name: b.name.clone(),
                    init,
                });
                Top::Caf {
                    global: b.name.clone(),
                }
            };
            self.tops.insert(b.name.clone(), top);
        }
        let evaluated = core::ww::whnf_params(p);
        for b in &p.binds {
            let (params, body) = lam_chain(&b.expr);
            let name = match &self.tops[&b.name] {
                Top::Fun { mir, .. } | Top::Const { mir } => mir.clone(),
                Top::Caf { global } => format!("{global}$init"),
            };
            let whnf = evaluated
                .get(&b.name)
                .filter(|e| e.len() == params.len())
                .cloned()
                .unwrap_or_default();
            self.pending.push(Pending {
                name,
                env: Env::new(),
                fvs: vec![],
                params,
                body: body.clone(),
                closure: false,
                whnf,
            });
        }
        while let Some(job) = self.pending.pop() {
            let f = self.function(job);
            self.m.funcs.push(f);
        }
        self.m.funcs.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Result type of a function type after `n` arguments.
    fn result_type(&self, t: &Type, n: usize) -> Type {
        let mut t = strip_fun(t).clone();
        for _ in 0..n {
            match t {
                Type::Fun(_, _, r) => t = strip_fun(&r).clone(),
                _ => return Type::Any,
            }
        }
        t
    }

    fn function(&mut self, job: Pending) -> Function {
        let sig = self.sigs.get(&job.name).cloned();
        let ret = match &sig {
            Some(s) => s.ret,
            None => Ty::Ptr,
        };
        let mut fb = Fb::new(&job.name, ret);
        if let Some(rs) = self.multi.get(&job.name) {
            fb.f.ret = rs[0];
            fb.f.rets = rs.clone();
        }
        let mut env = job.env;
        for (id, ty, whnf) in &job.fvs {
            let r = self.rep(ty);
            let v = fb.f.new_var(id.name.clone(), r);
            fb.f.params.push(ir::Param {
                var: v,
                own: Own::Owned,
            });
            env.insert(
                id.id,
                Bound::Val {
                    op: Operand::Var(v),
                    ty: ty.clone(),
                    // A captured value stays evaluated (BUG-0020).
                    whnf: !r.is_ptr() || *whnf,
                },
            );
        }
        let mut unbox = Vec::new();
        for (i, b) in job.params.iter().enumerate() {
            let native = self.rep(&b.ty);
            let r = if job.closure { Ty::Ptr } else { native };
            let v = fb.f.new_var(b.id.name.clone(), r);
            fb.f.params.push(ir::Param {
                var: v,
                own: Own::Owned,
            });
            if job.closure && native != Ty::Ptr {
                unbox.push((b.clone(), v, native));
            } else {
                env.insert(
                    b.id.id,
                    Bound::Val {
                        op: Operand::Var(v),
                        ty: b.ty.clone(),
                        whnf: !r.is_ptr() || job.whnf.get(i) == Some(&true),
                    },
                );
            }
        }
        for (b, v, native) in unbox {
            let raw = self.unbox(&mut fb, Operand::Var(v), native);
            env.insert(
                b.id.id,
                Bound::Val {
                    op: raw,
                    ty: b.ty.clone(),
                    whnf: true,
                },
            );
        }
        self.tail(&mut fb, &mut env, &job.body, job.closure);
        fb.f
    }

    // ---- boxing ----

    fn box_val(&mut self, fb: &mut Fb, op: Operand, r: Ty) -> Operand {
        let c = match r {
            Ty::F64 => self.box_f,
            _ => self.box_i,
        };
        Operand::Var(fb.let_(
            "box",
            Ty::Ptr,
            Rhs::Alloc {
                ctor: c,
                args: vec![op],
                reuse: None,
            },
        ))
    }

    fn unbox(&mut self, fb: &mut Fb, op: Operand, r: Ty) -> Operand {
        let c = match r {
            Ty::F64 => self.box_f,
            _ => self.box_i,
        };
        let v = self.force(fb, op);
        let Operand::Var(v) = v else { return v };
        Operand::Var(fb.let_(
            "unbox",
            r,
            Rhs::Proj {
                obj: v,
                ctor: c,
                field: 0,
            },
        ))
    }

    fn force(&mut self, fb: &mut Fb, op: Operand) -> Operand {
        match op {
            Operand::Var(v) if fb.f.ty(v) == Ty::Ptr => {
                Operand::Var(fb.let_("ev", Ty::Ptr, Rhs::Force(v)))
            }
            o => o,
        }
    }

    /// Ensures an operand is a variable (for `switch` / `proj`).
    fn as_var(&mut self, fb: &mut Fb, op: Operand) -> Var {
        match op {
            Operand::Var(v) => v,
            Operand::Const(c) => {
                let t = c.ty();
                fb.let_("k", t, Rhs::Lit(c))
            }
        }
    }

    // ---- types of expressions ----

    fn type_of(&self, env: &Env, extra: &mut HashMap<u32, Type>, e: &Expr) -> Type {
        match e {
            Expr::Var(i) => extra
                .get(&i.id)
                .cloned()
                .or_else(|| env.get(&i.id).map(|b| b.ty().clone()))
                .unwrap_or(Type::Any),
            Expr::Global(g) => self.p.bind(g).map(|b| b.ty.clone()).unwrap_or(Type::Any),
            Expr::Lit(l) => l.ty(),
            Expr::Con(c, targs, _) => match self.p.ctor(c) {
                Some((d, _)) => Type::Con(d.name.clone(), targs.clone()),
                None => Type::Any,
            },
            Expr::App(f, _) => match self.type_of(env, extra, f) {
                Type::Fun(_, _, r) => *r,
                _ => Type::Any,
            },
            Expr::TyApp(f, t) => match self.type_of(env, extra, f) {
                Type::Forall(v, _, b) => b.subst(&v, t),
                other => other,
            },
            Expr::Lam(b, body) => {
                extra.insert(b.id.id, b.ty.clone());
                Type::fun(b.mult, b.ty.clone(), self.type_of(env, extra, body))
            }
            Expr::TyLam(v, k, body) => Type::Forall(
                v.clone(),
                k.clone(),
                Box::new(self.type_of(env, extra, body)),
            ),
            Expr::Let(Bind::NonRec(b, _), body) | Expr::LetStrict(b, _, body) => {
                extra.insert(b.id.id, b.ty.clone());
                self.type_of(env, extra, body)
            }
            Expr::Let(Bind::Rec(bs), body) => {
                for (b, _) in bs {
                    extra.insert(b.id.id, b.ty.clone());
                }
                self.type_of(env, extra, body)
            }
            Expr::Case(_, _, t, _) | Expr::Cast(_, t) | Expr::Panic(_, t) => t.clone(),
            Expr::Prim(op, targs, _) => op.signature(targs).1,
            Expr::Extern(f, _) => self
                .p
                .extern_decl(f)
                .map(|d| d.ret.clone())
                .unwrap_or(Type::Any),
        }
    }

    // ---- free variables ----

    /// Free local variables of `e` (Known functions expand to their own
    /// free variables), in first-occurrence order.
    fn free_vars(&self, env: &Env, e: &Expr) -> Vec<Id> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut bound = HashSet::new();
        self.fv(env, e, &mut bound, &mut seen, &mut out);
        out
    }

    fn fv(
        &self,
        env: &Env,
        e: &Expr,
        bound: &mut HashSet<u32>,
        seen: &mut HashSet<u32>,
        out: &mut Vec<Id>,
    ) {
        let add = |i: &Id, out: &mut Vec<Id>, seen: &mut HashSet<u32>| {
            if seen.insert(i.id) {
                out.push(i.clone());
            }
        };
        match e {
            Expr::Var(i) => {
                if bound.contains(&i.id) {
                    return;
                }
                match env.get(&i.id) {
                    Some(Bound::Known { fvs, .. }) => {
                        for f in fvs.clone() {
                            add(&f, out, seen);
                        }
                    }
                    _ => add(i, out, seen),
                }
            }
            Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
            Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => {
                for x in a {
                    self.fv(env, x, bound, seen, out);
                }
            }
            Expr::App(f, a) => {
                self.fv(env, f, bound, seen, out);
                self.fv(env, a, bound, seen, out);
            }
            Expr::TyApp(e, _) | Expr::TyLam(_, _, e) | Expr::Cast(e, _) => {
                self.fv(env, e, bound, seen, out)
            }
            Expr::Lam(b, body) => {
                bound.insert(b.id.id);
                self.fv(env, body, bound, seen, out);
            }
            Expr::Let(Bind::NonRec(b, r), body) | Expr::LetStrict(b, r, body) => {
                self.fv(env, r, bound, seen, out);
                bound.insert(b.id.id);
                self.fv(env, body, bound, seen, out);
            }
            Expr::Let(Bind::Rec(bs), body) => {
                for (b, _) in bs {
                    bound.insert(b.id.id);
                }
                for (_, r) in bs {
                    self.fv(env, r, bound, seen, out);
                }
                self.fv(env, body, bound, seen, out);
            }
            Expr::Case(s, b, _, alts) => {
                self.fv(env, s, bound, seen, out);
                bound.insert(b.id.id);
                for a in alts {
                    if let AltCon::Con(_, bs) = &a.con {
                        for b in bs {
                            bound.insert(b.id.id);
                        }
                    }
                    self.fv(env, &a.rhs, bound, seen, out);
                }
            }
        }
    }

    /// Environment and captured variables for a function lifted out of
    /// `env` with free variables `fvs`.
    #[allow(clippy::type_complexity)]
    fn lifted_env(&self, env: &Env, fvs: &[Id]) -> (Env, Vec<(Id, Type, bool)>, Vec<Operand>) {
        let mut new_env = Env::new();
        // Known functions stay known (their own free variables are among `fvs`).
        for (k, b) in env {
            if let Bound::Known { .. } = b {
                new_env.insert(*k, b.clone());
            }
        }
        let mut caps = Vec::new();
        let mut ops = Vec::new();
        for f in fvs {
            match env.get(&f.id) {
                Some(Bound::Val {
                    op: Operand::Const(c),
                    ty,
                    whnf,
                }) => {
                    new_env.insert(
                        f.id,
                        Bound::Val {
                            op: Operand::Const(c.clone()),
                            ty: ty.clone(),
                            whnf: *whnf,
                        },
                    );
                }
                Some(Bound::Val { op, ty, whnf }) => {
                    caps.push((f.clone(), ty.clone(), *whnf));
                    ops.push(op.clone());
                }
                _ => {}
            }
        }
        (new_env, caps, ops)
    }

    // ---- expressions ----

    /// Lowers `e` in tail position: emits `ret` / `tailcall`.
    fn tail(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr, boxed: bool) {
        match e {
            Expr::Let(..) | Expr::LetStrict(..) => {
                if let Some(body) = self.let_(fb, env, e) {
                    self.tail(fb, env, body, boxed);
                }
            }
            Expr::Case(s, b, res, alts) => {
                self.case(fb, env, s, b, res, alts, Mode::Tail(boxed));
            }
            Expr::Cast(x, _) | Expr::TyLam(_, _, x) => self.tail(fb, env, x, boxed),
            Expr::Panic(m, _) if m == core::expr::UNREACHABLE => fb.term(Terminator::Unreachable),
            Expr::Panic(m, _) => fb.term(Terminator::Panic(m.clone())),
            // The fields of an unboxed tuple are the function's results
            // (lazy like constructor fields).
            Expr::Con(c, _, args) if core::cpr::is_utup(c) => {
                let mut ops = Vec::new();
                for (a, r) in args.iter().zip(fb.f.rets.clone()) {
                    let o = if r == Ty::Ptr {
                        self.lazy(fb, env, a)
                    } else {
                        self.whnf(fb, env, a)
                    };
                    let Some(o) = o else { return };
                    ops.push(o);
                }
                fb.term(Terminator::RetMulti(ops));
            }
            Expr::App(..) | Expr::TyApp(..) if !boxed => {
                if self.try_tail_call(fb, env, e) {
                    return;
                }
                if let Some(v) = self.whnf(fb, env, e) {
                    fb.term(Terminator::Ret(v));
                }
            }
            _ => {
                let Some(v) = self.whnf(fb, env, e) else {
                    return;
                };
                let r = self.op_ty(fb, &v);
                let v = if boxed && r != Ty::Ptr {
                    self.box_val(fb, v, r)
                } else {
                    v
                };
                fb.term(Terminator::Ret(v));
            }
        }
    }

    fn op_ty(&self, fb: &Fb, o: &Operand) -> Ty {
        match o {
            Operand::Var(v) => fb.f.ty(*v),
            Operand::Const(c) => c.ty(),
        }
    }

    /// Saturated call of a known function in tail position.
    fn try_tail_call(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr) -> bool {
        let (head, args) = spine(e);
        let Some((func, fvs, arity)) = self.known(env, head) else {
            return false;
        };
        if args.len() != arity
            || self.sigs.get(&func).map(|s| s.ret) != Some(fb.f.ret)
            || self.multi.get(&func) != self.multi.get(&fb.f.name)
        {
            return false;
        }
        let Some(ops) = self.call_args(fb, env, &func, &fvs, &args) else {
            return true;
        };
        fb.term(Terminator::TailCall { func, args: ops });
        true
    }

    /// A known function at the head of an application.
    fn known(&self, env: &Env, head: &Expr) -> Option<(String, Vec<Operand>, usize)> {
        match head {
            Expr::Global(g) => match self.tops.get(g) {
                Some(Top::Fun { mir, arity }) => Some((mir.clone(), vec![], *arity)),
                _ => None,
            },
            Expr::Var(i) => match env.get(&i.id) {
                Some(Bound::Known {
                    func, fvs, arity, ..
                }) => {
                    let ops = fvs
                        .iter()
                        .filter_map(|f| match env.get(&f.id) {
                            Some(Bound::Val { op, .. }) => Some(op.clone()),
                            _ => None,
                        })
                        .collect();
                    Some((func.clone(), ops, *arity))
                }
                _ => None,
            },
            Expr::Cast(x, _) => self.known(env, x),
            _ => None,
        }
    }

    /// Lowers the arguments of a direct call (native representations).
    fn call_args(
        &mut self,
        fb: &mut Fb,
        env: &mut Env,
        func: &str,
        fvs: &[Operand],
        args: &[&Expr],
    ) -> Option<Vec<Operand>> {
        let sig = self.sigs.get(func).cloned()?;
        let mut ops: Vec<Operand> = fvs.to_vec();
        for (i, a) in args.iter().enumerate() {
            let r = sig.params.get(fvs.len() + i).copied().unwrap_or(Ty::Ptr);
            let o = if r == Ty::Ptr {
                self.lazy(fb, env, a)?
            } else {
                self.whnf(fb, env, a)?
            };
            ops.push(o);
        }
        Some(ops)
    }

    /// Lowers `e` for a lazy position: atoms and values directly, other
    /// boxed expressions as thunks, unboxed expressions eagerly.
    fn lazy(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr) -> Option<Operand> {
        match e {
            Expr::Var(i) => match env.get(&i.id).cloned() {
                Some(Bound::Val { op, .. }) => Some(op),
                Some(Bound::Known { .. }) => self.closure_of(fb, env, e),
                None => {
                    self.error(format!("ICE: unbound variable `{}` in Core → MIR", i.name));
                    Some(Operand::Const(Const::Int(0, Ty::Ptr)))
                }
            },
            Expr::Global(g) if matches!(self.tops.get(g), Some(Top::Caf { .. })) => {
                // The CAF itself (evaluated on first use).
                let Some(Top::Caf { global }) = self.tops.get(g).cloned() else {
                    unreachable!()
                };
                Some(Operand::Var(fb.let_("caf", Ty::Ptr, Rhs::Global(global))))
            }
            Expr::Lit(_) | Expr::Lam(..) | Expr::Global(_) => self.whnf(fb, env, e),
            Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) => self.lazy(fb, env, x),
            Expr::Con(c, _, args) if self.is_newtype_ctor(c) => self.lazy(fb, env, &args[0]),
            Expr::Con(c, _, args) if self.builds_now(fb, env, c, args) => self.whnf(fb, env, e),
            _ => {
                let mut extra = HashMap::new();
                let t = self.type_of(env, &mut extra, e);
                if self.rep(&t) != Ty::Ptr {
                    return self.whnf(fb, env, e);
                }
                Some(self.thunk(fb, env, e))
            }
        }
    }

    /// Whether `lazy` lowers `e` to an evaluated value (not a thunk / CAF).
    fn builds_value(&self, fb: &Fb, env: &Env, e: &Expr) -> bool {
        match e {
            Expr::Lit(_) | Expr::Lam(..) => true,
            Expr::Con(c, _, args) if self.is_newtype_ctor(c) => {
                self.builds_value(fb, env, &args[0])
            }
            Expr::Con(c, _, args) => self.builds_now(fb, env, c, args),
            Expr::Global(g) => matches!(self.tops.get(g), Some(Top::Fun { .. })),
            Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) => {
                self.builds_value(fb, env, x)
            }
            _ => false,
        }
    }

    /// Whether a constructor in a lazy position can be built right away.
    /// Building it evaluates its strict fields, which nothing has demanded
    /// yet: they must be safe to compute now (BUG-0014).
    fn builds_now(&self, fb: &Fb, env: &Env, c: &str, args: &[Expr]) -> bool {
        let Some((_, strict)) = self.ctors.get(c) else {
            return true;
        };
        args.iter()
            .zip(strict)
            .all(|(a, st)| !*st || self.safe_now(fb, env, a))
    }

    /// Whether computing `e` now can neither fail nor run long. A variable
    /// counts when it is known to be evaluated (BUG-0020): a parameter or a
    /// captured variable may be a thunk that nothing has demanded yet.
    fn safe_now(&self, fb: &Fb, env: &Env, e: &Expr) -> bool {
        match e {
            Expr::Lit(_) | Expr::Lam(..) => true,
            Expr::Var(i) => match env.get(&i.id) {
                Some(Bound::Val {
                    op: Operand::Var(_),
                    whnf: false,
                    ..
                }) => false,
                Some(_) => true,
                None => false,
            },
            Expr::Global(g) => matches!(self.tops.get(g), Some(Top::Fun { .. })),
            Expr::TyApp(x, _) | Expr::TyLam(_, _, x) | Expr::Cast(x, _) => {
                self.safe_now(fb, env, x)
            }
            Expr::Con(c, _, args) if self.is_newtype_ctor(c) => self.safe_now(fb, env, &args[0]),
            Expr::Con(c, _, args) => self.builds_now(fb, env, c, args),
            Expr::Prim(op, _, args) => {
                core::ww::total_prim(*op) && args.iter().all(|a| self.safe_now(fb, env, a))
            }
            _ => false,
        }
    }

    fn is_newtype_ctor(&self, c: &str) -> bool {
        self.p.ctor(c).is_some_and(|(d, _)| d.newtype)
    }

    fn thunk(&mut self, fb: &mut Fb, env: &Env, e: &Expr) -> Operand {
        let fvs = self.free_vars(env, e);
        let (new_env, caps, ops) = self.lifted_env(env, &fvs);
        let name = self.fresh_name(&fb.f.name.clone(), "thk");
        let params: Vec<Ty> = caps.iter().map(|(_, t, _)| self.rep(t)).collect();
        self.sigs.insert(
            name.clone(),
            Sig {
                params,
                ret: Ty::Ptr,
            },
        );
        self.pending.push(Pending {
            name: name.clone(),
            env: new_env,
            fvs: caps,
            params: vec![],
            body: e.clone(),
            closure: false,
            whnf: vec![],
        });
        Operand::Var(fb.let_(
            "thk",
            Ty::Ptr,
            Rhs::MkThunk {
                func: name,
                args: ops,
            },
        ))
    }

    /// Lowers `e` to a value in WHNF; `None` if it diverges.
    fn whnf(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr) -> Option<Operand> {
        match e {
            Expr::Var(i) => match env.get(&i.id).cloned() {
                Some(Bound::Val { op, whnf, .. }) => {
                    if whnf {
                        Some(op)
                    } else {
                        let v = self.force(fb, op);
                        // Later uses see the evaluated value.
                        if let Some(Bound::Val { op, whnf, .. }) = env.get_mut(&i.id) {
                            *op = v.clone();
                            *whnf = true;
                        }
                        Some(v)
                    }
                }
                Some(Bound::Known { .. }) => self.closure_of(fb, env, e),
                None => {
                    self.error(format!("ICE: unbound variable `{}` in Core → MIR", i.name));
                    Some(Operand::Const(Const::Int(0, Ty::I64)))
                }
            },
            Expr::Global(g) => match self.tops.get(g).cloned() {
                Some(Top::Fun { .. }) => self.closure_of(fb, env, e),
                Some(Top::Caf { global }) => {
                    let t = fb.let_("caf", Ty::Ptr, Rhs::Global(global));
                    Some(Operand::Var(fb.let_("ev", Ty::Ptr, Rhs::Force(t))))
                }
                Some(Top::Const { mir }) => {
                    let r = self.sigs[&mir].ret;
                    Some(Operand::Var(fb.let_(
                        "k",
                        r,
                        Rhs::Call {
                            func: mir,
                            args: vec![],
                        },
                    )))
                }
                None => {
                    self.error(format!("ICE: unknown global `{g}`"));
                    None
                }
            },
            Expr::Lit(l) => Some(match l {
                Lit::Int(n) => Operand::Const(Const::Int(*n, Ty::I64)),
                Lit::Double(x) => Operand::Const(Const::Float(*x)),
                Lit::Str(s) => Operand::Const(Const::Str(s.clone())),
                Lit::Unit => Operand::Const(Const::Int(0, Ty::I64)),
            }),
            Expr::Con(c, _, args) => {
                if self.is_newtype_ctor(c) {
                    return self.whnf(fb, env, &args[0]);
                }
                let Some((cr, strict)) = self.ctors.get(c).cloned() else {
                    self.error(format!("ICE: unknown constructor `{c}`"));
                    return None;
                };
                let mut ops = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let o = if strict.get(i) == Some(&true) {
                        self.whnf(fb, env, a)?
                    } else {
                        self.lazy(fb, env, a)?
                    };
                    ops.push(o);
                }
                Some(Operand::Var(fb.let_(
                    "con",
                    Ty::Ptr,
                    Rhs::Alloc {
                        ctor: cr,
                        args: ops,
                        reuse: None,
                    },
                )))
            }
            Expr::Lam(..) => Some(self.lambda_closure(fb, env, e)),
            Expr::TyLam(_, _, x) | Expr::Cast(x, _) => self.whnf(fb, env, x),
            Expr::App(..) | Expr::TyApp(..) => self.app(fb, env, e),
            Expr::Let(..) | Expr::LetStrict(..) => {
                let body = self.let_(fb, env, e)?;
                self.whnf(fb, env, body)
            }
            Expr::Case(s, b, res, alts) => self.case(fb, env, s, b, res, alts, Mode::Value),
            Expr::Prim(op, _, args) => self.prim(fb, env, *op, args),
            Expr::Extern(f, args) => {
                let decl = self.m.extern_decl(f).cloned()?;
                let mut ops = Vec::new();
                for a in args {
                    ops.push(self.whnf(fb, env, a)?);
                }
                match decl.ret {
                    None => {
                        fb.emit(Instr::Let {
                            var: None,
                            rhs: Rhs::Extern {
                                func: f.clone(),
                                args: ops,
                            },
                        });
                        Some(Operand::Const(Const::Int(0, Ty::I64)))
                    }
                    Some(r) => Some(Operand::Var(fb.let_(
                        "ext",
                        r,
                        Rhs::Extern {
                            func: f.clone(),
                            args: ops,
                        },
                    ))),
                }
            }
            Expr::Panic(m, _) => {
                fb.term(if m == core::expr::UNREACHABLE {
                    Terminator::Unreachable
                } else {
                    Terminator::Panic(m.clone())
                });
                None
            }
        }
    }

    fn prim(&mut self, fb: &mut Fb, env: &mut Env, op: PrimOp, args: &[Expr]) -> Option<Operand> {
        use ir::Prim as P;
        let mp = match op {
            PrimOp::Add => P::Add,
            PrimOp::Sub => P::Sub,
            PrimOp::Mul => P::Mul,
            PrimOp::Quot => P::Div,
            PrimOp::Rem => P::Rem,
            PrimOp::QuotNz => P::DivNz,
            PrimOp::RemNz => P::RemNz,
            PrimOp::ArrayGetUnchecked => P::ArrayGetUnchecked,
            PrimOp::Neg => P::Neg,
            PrimOp::And => P::And,
            PrimOp::Or => P::Or,
            PrimOp::Xor => P::Xor,
            PrimOp::Shl => P::Shl,
            PrimOp::Shr => P::Shr,
            PrimOp::Eq => P::Eq,
            PrimOp::Ne => P::Ne,
            PrimOp::Lt => P::Lt,
            PrimOp::Le => P::Le,
            PrimOp::Gt => P::Gt,
            PrimOp::Ge => P::Ge,
            PrimOp::FAdd => P::FAdd,
            PrimOp::FSub => P::FSub,
            PrimOp::FMul => P::FMul,
            PrimOp::FDiv => P::FDiv,
            PrimOp::FNeg => P::FNeg,
            PrimOp::FEq => P::FEq,
            PrimOp::FLt => P::FLt,
            PrimOp::FLe => P::FLe,
            PrimOp::IntToDouble => P::I64ToF64,
            PrimOp::DoubleToInt => P::F64ToI64,
            PrimOp::BytesNew => P::BytesNew,
            PrimOp::BytesLen => P::BytesLen,
            PrimOp::BytesGet => P::BytesGet,
            PrimOp::BytesSet => P::BytesSet,
            PrimOp::ArrayNew => P::ArrayNew,
            PrimOp::ArrayLen => P::ArrayLen,
            PrimOp::ArrayGet => P::ArrayGet,
            PrimOp::ArraySet => P::ArraySet,
            PrimOp::ArrayUnique => P::ArrayUnique,
            PrimOp::BytesGet64 => P::BytesGetI64,
            PrimOp::BytesSet64 => P::BytesSetI64,
            PrimOp::BytesGetF => P::BytesGetF64,
            PrimOp::BytesSetF => P::BytesSetF64,
            PrimOp::BytesUnique => P::BytesUnique,
            PrimOp::ArraySetShared => P::ArraySetShared,
            PrimOp::BytesSet64Shared => P::BytesSetI64Shared,
            PrimOp::BytesSetFShared => P::BytesSetF64Shared,
            PrimOp::PtrEq => P::PtrEq,
        };
        let mut ops = Vec::new();
        for (i, a) in args.iter().enumerate() {
            // Array elements are stored lazily.
            let lazy_elem = matches!(
                (op, i),
                (PrimOp::ArrayNew, 1) | (PrimOp::ArraySet | PrimOp::ArraySetShared, 2)
            );
            ops.push(if lazy_elem {
                self.lazy(fb, env, a)?
            } else {
                self.whnf(fb, env, a)?
            });
        }
        let r = mp.signature().1;
        let v = fb.let_("p", r, Rhs::Prim(mp, ops));
        if matches!(op, PrimOp::ArrayGet | PrimOp::ArrayGetUnchecked) {
            // Elements may be thunks.
            return Some(self.force(fb, Operand::Var(v)));
        }
        Some(Operand::Var(v))
    }

    /// Processes the bindings of a `let` / `let!` / `letrec` and returns
    /// the body (`None` if a strict right-hand side diverges).
    fn let_<'e>(&mut self, fb: &mut Fb, env: &mut Env, e: &'e Expr) -> Option<&'e Expr> {
        match e {
            Expr::LetStrict(b, rhs, body) => {
                let v = self.whnf(fb, env, rhs)?;
                env.insert(
                    b.id.id,
                    Bound::Val {
                        op: v,
                        ty: b.ty.clone(),
                        whnf: true,
                    },
                );
                Some(body)
            }
            Expr::Let(Bind::NonRec(b, rhs), body) => {
                // A variable, maybe under newtype constructors (FEAT-0066).
                let mut atom = &**rhs;
                while let Expr::Con(c, _, args) = atom
                    && self.is_newtype_ctor(c)
                {
                    atom = &args[0];
                }
                let atomic = matches!(atom, Expr::Var(_));
                let v = self.lazy(fb, env, rhs)?;
                let whnf = if atomic {
                    match atom {
                        Expr::Var(i) => {
                            matches!(env.get(&i.id), Some(Bound::Val { whnf: true, .. }))
                        }
                        _ => false,
                    }
                } else {
                    // Values are built eagerly; other boxed rhs are thunks.
                    self.op_ty(fb, &v) != Ty::Ptr
                        || matches!(&v, Operand::Const(_))
                        || self.builds_value(fb, env, rhs)
                };
                env.insert(
                    b.id.id,
                    Bound::Val {
                        op: v,
                        ty: b.ty.clone(),
                        whnf,
                    },
                );
                Some(body)
            }
            Expr::Let(Bind::Rec(bs), body) => {
                self.letrec(fb, env, bs);
                Some(body)
            }
            _ => Some(e),
        }
    }

    fn letrec(&mut self, fb: &mut Fb, env: &mut Env, bs: &[(core::Binder, Expr)]) {
        if bs.iter().any(|(_, rhs)| lam_chain(rhs).0.is_empty()) {
            self.letrec_values(fb, env, bs);
            return;
        }
        // Free variables of the whole group (excluding the group itself).
        let group: HashSet<u32> = bs.iter().map(|(b, _)| b.id.id).collect();
        let mut fvs: Vec<Id> = Vec::new();
        for (_, rhs) in bs {
            for v in self.free_vars(env, rhs) {
                if !group.contains(&v.id) && !fvs.contains(&v) {
                    fvs.push(v);
                }
            }
        }
        let base = fb.f.name.clone();
        let mut names = Vec::new();
        for (b, rhs) in bs {
            let name = self.fresh_name(&base, &b.id.name);
            let (params, _) = lam_chain(rhs);
            env.insert(
                b.id.id,
                Bound::Known {
                    func: name.clone(),
                    fvs: fvs.clone(),
                    arity: params.len(),
                    ty: b.ty.clone(),
                },
            );
            names.push(name);
        }
        let (new_env, caps, _) = self.lifted_env(env, &fvs);
        let fv_tys: Vec<Ty> = caps.iter().map(|(_, t, _)| self.rep(t)).collect();
        // Known entries must only capture what is passed.
        let cap_ids: Vec<Id> = caps.iter().map(|(i, _, _)| i.clone()).collect();
        for (b, _) in bs {
            if let Some(Bound::Known { fvs, .. }) = env.get_mut(&b.id.id) {
                *fvs = cap_ids.clone();
            }
        }
        let mut new_env = new_env;
        for (b, _) in bs {
            new_env.insert(b.id.id, env[&b.id.id].clone());
        }
        for ((b, rhs), name) in bs.iter().zip(names) {
            let (params, body) = lam_chain(rhs);
            let mut ps = fv_tys.clone();
            ps.extend(params.iter().map(|x| self.rep(&x.ty)));
            let ret = self.rep(&self.result_type(&b.ty, params.len()));
            self.sigs.insert(name.clone(), Sig { params: ps, ret });
            self.pending.push(Pending {
                name,
                env: new_env.clone(),
                fvs: caps.clone(),
                params,
                body: body.clone(),
                closure: false,
                whnf: vec![],
            });
        }
    }

    /// A recursive group with value bindings (knot-tying, e.g.
    /// `let xs = 1 : xs`; 04-memory-model.md §5, FEAT-0126): every member
    /// becomes a thunk. The thunks are allocated with placeholders for the
    /// members they capture, which `thunk_init` then fills in, and are
    /// recorded as roots of the cycle collector.
    fn letrec_values(&mut self, fb: &mut Fb, env: &mut Env, bs: &[(core::Binder, Expr)]) {
        for (b, _) in bs {
            if self.rep(&b.ty) != Ty::Ptr {
                self.error(format!(
                    "recursive binding `{}` of an unboxed type is not supported",
                    b.id.name
                ));
                return;
            }
        }
        let group: HashSet<u32> = bs.iter().map(|(b, _)| b.id.id).collect();
        // Placeholder variables (never defined) stand for the members while
        // the thunks are built.
        let mut holders: HashMap<Var, usize> = HashMap::new();
        for (j, (b, _)) in bs.iter().enumerate() {
            let v = fb.f.fresh_var(&b.id.name, Ty::Ptr);
            holders.insert(v, j);
            env.insert(
                b.id.id,
                Bound::Val {
                    op: Operand::Var(v),
                    ty: b.ty.clone(),
                    whnf: false,
                },
            );
        }
        let base = fb.f.name.clone();
        let mut thunks = Vec::new();
        let mut inits = Vec::new();
        for (i, (b, rhs)) in bs.iter().enumerate() {
            // Members first: their slots are then preceded by pointers only.
            let mut fvs = self.free_vars(env, rhs);
            fvs.sort_by_key(|f| !group.contains(&f.id));
            let (new_env, caps, mut ops) = self.lifted_env(env, &fvs);
            for (k, op) in ops.iter_mut().enumerate() {
                if let Operand::Var(v) = op
                    && let Some(&j) = holders.get(v)
                {
                    // A static immortal object: nothing to count.
                    *op = Operand::Const(Const::Str(Vec::new()));
                    inits.push((i, k, j));
                }
            }
            let name = self.fresh_name(&base, &b.id.name);
            let params: Vec<Ty> = caps.iter().map(|(_, t, _)| self.rep(t)).collect();
            self.sigs.insert(
                name.clone(),
                Sig {
                    params,
                    ret: Ty::Ptr,
                },
            );
            self.pending.push(Pending {
                name: name.clone(),
                env: new_env,
                fvs: caps,
                params: vec![],
                body: rhs.clone(),
                closure: false,
                whnf: vec![],
            });
            thunks.push(fb.let_(
                &b.id.name,
                Ty::Ptr,
                Rhs::MkThunk {
                    func: name,
                    args: ops,
                },
            ));
        }
        for (i, k, j) in inits {
            fb.let_(
                "init",
                Ty::I64,
                Rhs::Prim(
                    ir::Prim::ThunkInit,
                    vec![
                        Operand::Var(thunks[i]),
                        Operand::Const(Const::Int(k as i64, Ty::I64)),
                        Operand::Var(thunks[j]),
                    ],
                ),
            );
        }
        for (t, (b, _)) in thunks.iter().zip(bs) {
            fb.let_(
                "cand",
                Ty::I64,
                Rhs::Prim(ir::Prim::CycleCandidate, vec![Operand::Var(*t)]),
            );
            env.insert(
                b.id.id,
                Bound::Val {
                    op: Operand::Var(*t),
                    ty: b.ty.clone(),
                    whnf: false,
                },
            );
        }
    }

    /// A closure for a lambda expression (closure convention).
    fn lambda_closure(&mut self, fb: &mut Fb, env: &Env, e: &Expr) -> Operand {
        let (mut params, mut body) = lam_chain(e);
        let mut rest_body;
        if params.len() > ir::MAX_CLOSURE_ARITY {
            // Split: the first 8 parameters return a closure for the rest.
            let rest = params.split_off(ir::MAX_CLOSURE_ARITY);
            rest_body = body.clone();
            for b in rest.into_iter().rev() {
                rest_body = Expr::Lam(b, Box::new(rest_body));
            }
            body = &rest_body;
        }
        let fvs = self.free_vars(env, e);
        let (new_env, caps, ops) = self.lifted_env(env, &fvs);
        let name = self.fresh_name(&fb.f.name.clone(), "lam");
        let mut ps: Vec<Ty> = caps.iter().map(|(_, t, _)| self.rep(t)).collect();
        ps.extend(params.iter().map(|_| Ty::Ptr));
        self.sigs.insert(
            name.clone(),
            Sig {
                params: ps,
                ret: Ty::Ptr,
            },
        );
        self.pending.push(Pending {
            name: name.clone(),
            env: new_env,
            fvs: caps,
            params,
            body: body.clone(),
            closure: true,
            whnf: vec![],
        });
        Operand::Var(fb.let_(
            "clo",
            Ty::Ptr,
            Rhs::MkClosure {
                func: name,
                args: ops,
            },
        ))
    }

    /// A closure for a known function used as a value.
    fn closure_of(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr) -> Option<Operand> {
        let (func, fvs, arity) = self.known(env, e)?;
        Some(self.pap(fb, &func, fvs, arity, vec![]))
    }

    /// `mkclosure` of `func` capturing `fvs` and already-evaluated pointer
    /// arguments `given` (closure-convention wrapper when needed).
    fn pap(
        &mut self,
        fb: &mut Fb,
        func: &str,
        fvs: Vec<Operand>,
        arity: usize,
        given: Vec<Operand>,
    ) -> Operand {
        let sig = self.sigs[func].clone();
        let nfv = fvs.len();
        let native_ok = sig.ret == Ty::Ptr && sig.params[nfv..].iter().all(|t| *t == Ty::Ptr);
        let target = if native_ok && arity - given.len() <= ir::MAX_CLOSURE_ARITY {
            func.to_string()
        } else {
            self.wrapper(func, nfv)
        };
        if arity - given.len() > ir::MAX_CLOSURE_ARITY {
            self.error(format!(
                "partial application of `{func}` leaves more than {} parameters",
                ir::MAX_CLOSURE_ARITY
            ));
        }
        let mut args = fvs;
        args.extend(given);
        Operand::Var(fb.let_("clo", Ty::Ptr, Rhs::MkClosure { func: target, args }))
    }

    /// `func$clo(fvs..., p1: ptr, ...) -> ptr`: unboxes, calls, boxes.
    fn wrapper(&mut self, func: &str, nfv: usize) -> String {
        if let Some(w) = self.wrappers.get(func) {
            return w.clone();
        }
        let name = format!("{func}$clo");
        self.wrappers.insert(func.to_string(), name.clone());
        let sig = self.sigs[func].clone();
        let mut fb = Fb::new(&name, Ty::Ptr);
        let mut args = Vec::new();
        let mut unbox = Vec::new();
        for (i, t) in sig.params.iter().enumerate() {
            let pty = if i < nfv { *t } else { Ty::Ptr };
            let v = fb.f.new_var(format!("a{i}"), pty);
            fb.f.params.push(ir::Param {
                var: v,
                own: Own::Owned,
            });
            if i >= nfv && *t != Ty::Ptr {
                unbox.push((args.len(), v, *t));
            }
            args.push(Operand::Var(v));
        }
        for (k, v, t) in unbox {
            args[k] = self.unbox(&mut fb, Operand::Var(v), t);
        }
        let r = fb.let_(
            "r",
            sig.ret,
            Rhs::Call {
                func: func.to_string(),
                args,
            },
        );
        let out = if sig.ret != Ty::Ptr {
            self.box_val(&mut fb, Operand::Var(r), sig.ret)
        } else {
            Operand::Var(r)
        };
        fb.term(Terminator::Ret(out));
        let mut ps: Vec<Ty> = sig.params[..nfv].to_vec();
        ps.extend(std::iter::repeat_n(Ty::Ptr, sig.params.len() - nfv));
        self.sigs.insert(
            name.clone(),
            Sig {
                params: ps,
                ret: Ty::Ptr,
            },
        );
        self.m.funcs.push(fb.f);
        name
    }

    fn app(&mut self, fb: &mut Fb, env: &mut Env, e: &Expr) -> Option<Operand> {
        let (head, args) = spine(e);
        if args.is_empty() {
            // Only type applications.
            return self.whnf(fb, env, head);
        }
        // Beta-redex: bind the parameters.
        if let Expr::Lam(..) = strip_tylams(head) {
            let (params, body) = lam_chain(head);
            let n = params.len().min(args.len());
            let mut inner = body.clone();
            for b in params[n..].iter().rev() {
                inner = Expr::Lam(b.clone(), Box::new(inner));
            }
            for (b, a) in params.iter().zip(&args).take(n) {
                let v = if self.rep(&b.ty) == Ty::Ptr {
                    self.lazy(fb, env, a)?
                } else {
                    self.whnf(fb, env, a)?
                };
                let whnf = self.op_ty(fb, &v) != Ty::Ptr;
                env.insert(
                    b.id.id,
                    Bound::Val {
                        op: v,
                        ty: b.ty.clone(),
                        whnf,
                    },
                );
            }
            let rest: Vec<Expr> = args[n..].iter().map(|a| (*a).clone()).collect();
            let call = Expr::app(inner, rest);
            return self.whnf(fb, env, &call);
        }
        let mut extra = HashMap::new();
        let result_ty = self.type_of(env, &mut extra, e);
        if let Some((func, fvs, arity)) = self.known(env, head) {
            if args.len() < arity {
                let mut given = Vec::new();
                let sig = self.sigs[&func].clone();
                for (i, a) in args.iter().enumerate() {
                    let r = sig.params[fvs.len() + i];
                    let o = if r == Ty::Ptr {
                        self.lazy(fb, env, a)?
                    } else {
                        let v = self.whnf(fb, env, a)?;
                        self.box_val(fb, v, r)
                    };
                    given.push(o);
                }
                return Some(self.pap(fb, &func, fvs, arity, given));
            }
            let ops = self.call_args(fb, env, &func, &fvs, &args[..arity])?;
            let ret = self.sigs[&func].ret;
            let r = fb.let_("r", ret, Rhs::Call { func, args: ops });
            if args.len() == arity {
                return Some(Operand::Var(r));
            }
            return self.apply_rest(fb, env, Operand::Var(r), &args[arity..], &result_ty);
        }
        let f = self.whnf(fb, env, head)?;
        self.apply_rest(fb, env, f, &args, &result_ty)
    }

    /// Applies closure `f` to `args` (boxed) and unboxes the result if needed.
    fn apply_rest(
        &mut self,
        fb: &mut Fb,
        env: &mut Env,
        f: Operand,
        args: &[&Expr],
        result_ty: &Type,
    ) -> Option<Operand> {
        let mut ops = Vec::new();
        for a in args {
            let mut extra = HashMap::new();
            let t = self.type_of(env, &mut extra, a);
            let r = self.rep(&t);
            let o = if r == Ty::Ptr {
                self.lazy(fb, env, a)?
            } else {
                let v = self.whnf(fb, env, a)?;
                self.box_val(fb, v, r)
            };
            ops.push(o);
        }
        let mut clo = self.as_var(fb, f);
        let mut rest = ops.as_slice();
        loop {
            let n = rest.len().min(ir::MAX_CLOSURE_ARITY);
            let r = fb.let_(
                "ap",
                Ty::Ptr,
                Rhs::Apply {
                    clo,
                    args: rest[..n].to_vec(),
                },
            );
            rest = &rest[n..];
            clo = r;
            if rest.is_empty() {
                break;
            }
        }
        let r = self.rep(result_ty);
        if r != Ty::Ptr {
            return Some(self.unbox(fb, Operand::Var(clo), r));
        }
        Some(Operand::Var(clo))
    }

    #[allow(clippy::too_many_arguments)]
    /// `case f args of UTup<n> x̄ -> rhs`: a call with several results.
    fn case_multi(
        &mut self,
        fb: &mut Fb,
        env: &mut Env,
        scrut: &Expr,
        bs: &[core::Binder],
        rhs: &Expr,
        mode: Mode,
    ) -> Option<Operand> {
        // Bindings evaluated before the call (Core Lint allows them).
        let mut scrut = scrut;
        while matches!(scrut, Expr::Let(..) | Expr::LetStrict(..)) {
            scrut = self.let_(fb, env, scrut)?;
        }
        let (head, args) = spine(scrut);
        let known = self.known(env, head);
        let Some((func, fvs, _)) =
            known.filter(|(f, _, a)| *a == args.len() && self.multi.contains_key(f))
        else {
            self.error("ICE: an unboxed tuple taken apart from something else than a known call");
            return None;
        };
        let ops = self.call_args(fb, env, &func, &fvs, &args)?;
        let rs = self.multi[&func].clone();
        let vars: Vec<Var> = bs
            .iter()
            .zip(&rs)
            .map(|(b, r)| fb.f.new_var(b.id.name.clone(), *r))
            .collect();
        fb.emit(Instr::LetMulti {
            vars: vars.clone(),
            func,
            args: ops,
        });
        for ((b, v), r) in bs.iter().zip(vars).zip(rs) {
            env.insert(
                b.id.id,
                Bound::Val {
                    op: Operand::Var(v),
                    ty: b.ty.clone(),
                    whnf: r != Ty::Ptr,
                },
            );
        }
        match mode {
            Mode::Tail(boxed) => {
                self.tail(fb, env, rhs, boxed);
                None
            }
            Mode::Value => self.whnf(fb, env, rhs),
        }
    }

    fn case(
        &mut self,
        fb: &mut Fb,
        env: &mut Env,
        scrut: &Expr,
        b: &core::Binder,
        res: &Type,
        alts: &[core::Alt],
        mode: Mode,
    ) -> Option<Operand> {
        if let [
            core::Alt {
                con: AltCon::Con(c, bs),
                rhs,
            },
        ] = alts
            && core::cpr::is_utup(c)
        {
            return self.case_multi(fb, env, scrut, bs, rhs, mode);
        }
        let v = self.whnf(fb, env, scrut)?;
        env.insert(
            b.id.id,
            Bound::Val {
                op: v.clone(),
                ty: b.ty.clone(),
                whnf: true,
            },
        );
        let res_rep = self.rep(res);
        // Join point for value mode.
        let join = match mode {
            Mode::Value => {
                let j = fb.block("join");
                let p = fb.f.fresh_var("res", res_rep);
                fb.f.blocks[j.0 as usize].params.push(p);
                Some((j, p))
            }
            Mode::Tail(_) => None,
        };
        let newtype = match &b.ty {
            Type::Con(c, _) => self.datas.get(c).is_some_and(|d| d.newtype),
            _ => false,
        };
        let has_ctor_alts = alts.iter().any(|a| matches!(a.con, AltCon::Con(..)));
        let has_int_alts = alts.iter().any(|a| matches!(a.con, AltCon::Int(_)));
        let mut targets: Vec<(Option<ir::Case>, &core::Alt, Vec<(core::Binder, usize)>)> =
            Vec::new();
        for a in alts {
            match &a.con {
                AltCon::Default => targets.push((None, a, vec![])),
                AltCon::Int(n) => targets.push((Some(ir::Case::Int(*n)), a, vec![])),
                AltCon::Con(c, bs) => {
                    if newtype {
                        targets.push((
                            None,
                            a,
                            bs.iter().cloned().map(|b| (b, usize::MAX)).collect(),
                        ));
                    } else {
                        let cr = self.ctors[c].0;
                        targets.push((
                            Some(ir::Case::Ctor(cr)),
                            a,
                            bs.iter()
                                .cloned()
                                .enumerate()
                                .map(|(i, b)| (b, i))
                                .collect(),
                        ));
                    }
                }
            }
        }
        let emit_alt = |this: &mut Self,
                        fb: &mut Fb,
                        env: &mut Env,
                        a: &core::Alt,
                        fields: &[(core::Binder, usize)],
                        cr: Option<CtorRef>| {
            for (fbnd, i) in fields {
                if *i == usize::MAX {
                    // Newtype: the field is the value itself.
                    env.insert(
                        fbnd.id.id,
                        Bound::Val {
                            op: v.clone(),
                            ty: fbnd.ty.clone(),
                            whnf: true,
                        },
                    );
                    continue;
                }
                let cr = cr.unwrap();
                let obj = this.as_var(fb, v.clone());
                let fty = this.rep(&fbnd.ty);
                let strict = this
                    .ctors
                    .iter()
                    .find(|(_, (r, _))| *r == cr)
                    .map(|(_, (_, s))| s.get(*i).copied().unwrap_or(false))
                    .unwrap_or(false);
                let x = fb.let_(
                    &fbnd.id.name,
                    fty,
                    Rhs::Proj {
                        obj,
                        ctor: cr,
                        field: *i as u32,
                    },
                );
                env.insert(
                    fbnd.id.id,
                    Bound::Val {
                        op: Operand::Var(x),
                        ty: fbnd.ty.clone(),
                        whnf: strict || fty != Ty::Ptr,
                    },
                );
            }
            match mode {
                Mode::Tail(boxed) => {
                    this.tail(fb, env, &a.rhs, boxed);
                }
                Mode::Value => {
                    if let Some(r) = this.whnf(fb, env, &a.rhs) {
                        let (j, _) = join.unwrap();
                        fb.term(Terminator::Jump(j, vec![r]));
                    }
                }
            }
        };
        if (!has_ctor_alts || newtype) && !has_int_alts {
            // Only a default (or newtype) alternative: no switch.
            let (_, a, fields) = &targets[0];
            emit_alt(self, fb, env, a, fields, None);
        } else {
            let scrut_var = self.as_var(fb, v.clone());
            let start = fb.cur;
            let mut arms = Vec::new();
            let mut default = None;
            let mut blocks = Vec::new();
            for (case, a, fields) in &targets {
                let blk = fb.block("alt");
                match case {
                    Some(c) => arms.push((c.clone(), blk)),
                    None => default = Some(blk),
                }
                blocks.push((blk, *a, fields.clone(), case.clone()));
            }
            let complete = match (&b.ty, has_ctor_alts) {
                (Type::Con(c, _), true) => {
                    self.p.data(c).is_some_and(|d| d.ctors.len() == arms.len())
                }
                _ => false,
            };
            if default.is_none() && !complete {
                let blk = fb.block("nomatch");
                fb.switch_to(blk);
                fb.term(Terminator::Panic("incomplete pattern match".into()));
                default = Some(blk);
            }
            fb.cur = start;
            fb.term(Terminator::Switch {
                scrut: scrut_var,
                arms,
                default,
            });
            for (blk, a, fields, case) in blocks {
                fb.switch_to(blk);
                let mut env2 = env.clone();
                let cr = match case {
                    Some(ir::Case::Ctor(cr)) => Some(cr),
                    _ => None,
                };
                emit_alt(self, fb, &mut env2, a, &fields, cr);
            }
        }
        match join {
            Some((j, p)) => {
                if fb.f.predecessors()[j.0 as usize].is_empty() {
                    // Every alternative diverges.
                    return None;
                }
                fb.switch_to(j);
                Some(Operand::Var(p))
            }
            None => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Value,
    /// Tail position; `true` = box an unboxed result (closure convention).
    Tail(bool),
}
