//! achc-desugar: translation of the typed Haskell program (`achc-typeck`)
//! into the common Core (06-ir.md §1).
//!
//! - Types: `Int` / `Char` / `Double` are boxed (`I#` / `C#` / `D#`),
//!   `IO a` is a newtype of `Unit# -> a`, built-in syntax gets plain Core names
//!   (`List` with `Nil` / `Cons`, `Unit`, `TupleN`), and every other data
//!   type keeps its qualified Haskell name.
//! - Evaluation stays lazy: Core `let` and non-atomic arguments are thunks.
//! - Pattern matching is compiled into `case` trees column by column
//!   (Wadler's match compiler with the mixture rule). Failure continuations
//!   used once are inlined, others are shared through a lazy `let`.
//!   Reachable match failure is W0610, unreachable clauses are W0611.
//! - `foreign import prim "p"` binds to the Core definition `prim$p` of
//!   `prims.core`; the declared type must equal the primitive's type.

mod exhaust;
mod ffi;

use achc_typeck::infer::kind_of;
use achc_typeck::tast::{self, BindGroup, Clause, Guard, Pat};
use achc_typeck::types::*;
use cirka_core as c;
use cirka_diagnostics::{Diagnostic, Span};
use std::collections::{BTreeSet, HashMap};

#[cfg(test)]
mod tests;

const PRIMS: &str = include_str!("prims.core");

/// A desugared program with its warnings.
pub struct Output {
    pub program: c::Program,
    pub warnings: Vec<Diagnostic>,
}

/// Translates a type-checked program to Core. The result passes Core Lint
/// (a failure is an internal compiler error).
pub fn desugar(p: &tast::Program) -> Result<Output, Vec<Diagnostic>> {
    let prims = match c::parse_program(cirka_diagnostics::FileId(u32::MAX), PRIMS) {
        Ok(p) => p,
        Err(e) => return Err(vec![Diagnostic::ice(format!("prims.core: {}", e.message))]),
    };
    let mut d = Ds::new(p, prims.max_id() + 1);
    let mut prog = d.program(p, prims);
    if !d.errors.is_empty() {
        return Err(d.errors);
    }
    // Remove primitives the program does not use.
    prune(&mut prog);
    let errs = c::lint::lint_program(&prog);
    if !errs.is_empty() {
        return Err(errs
            .into_iter()
            .map(|e| Diagnostic::ice(format!("Core Lint after desugaring: {}", e.message)))
            .collect());
    }
    let mut warnings = p.warnings.clone();
    warnings.extend(d.warnings);
    Ok(Output {
        program: prog,
        warnings,
    })
}

#[derive(Clone)]
struct ConMeta {
    data: String,
    fields: Vec<Ty>,
}

struct DataMeta {
    params: Vec<String>,
    cons: Vec<String>,
    newtype: bool,
}

/// A matched column: the variable and its Haskell type.
type Col = (c::Id, Ty);

#[derive(Clone)]
enum RowRhs<'a> {
    Rhs(&'a tast::Rhs),
    /// The remaining guards of a pattern guard, then the body.
    Guards(&'a [Guard], &'a tast::Expr),
    Expr(c::Expr),
}

#[derive(Clone)]
struct Row<'a> {
    pats: Vec<Pat>,
    rhs: RowRhs<'a>,
    /// Lazily bound variables (irrefutable patterns).
    lets: Vec<(c::Binder, c::Expr)>,
    clause: Option<usize>,
}

struct Ds {
    next: u32,
    locals: HashMap<String, c::Id>,
    tyvars: HashMap<String, c::TyVar>,
    tuples: BTreeSet<usize>,
    cons: HashMap<String, ConMeta>,
    datas: HashMap<String, DataMeta>,
    /// Clauses of the match being compiled that produced code.
    reached: Vec<bool>,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
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

fn core_tname(n: &str) -> String {
    if n == list_tc() {
        return "List".into();
    }
    if n == unit_tc() {
        return "Unit".into();
    }
    if let Some(k) = tuple_arity(n) {
        return format!("Tuple{k}");
    }
    n.strip_prefix("Cirka.Prim.").unwrap_or(n).to_string()
}

fn core_cname(n: &str) -> String {
    if n == list_tc() {
        return "Nil".into();
    }
    if n == prim(":") {
        return "Cons".into();
    }
    core_tname(n)
}

fn local_name(n: &str) -> String {
    let n = achc_rename::display_name(n);
    let t = n
        .trim_start_matches('$')
        .trim_end_matches(|c: char| c.is_ascii_digit());
    if t.is_empty() {
        "v".into()
    } else {
        t.to_string()
    }
}

fn is_true(e: &tast::Expr) -> bool {
    match e {
        tast::Expr::Con(c, _) => c == &prim("True"),
        tast::Expr::Global(g, _) => {
            g == "Prelude.otherwise" || *g == format!("{}.otherwise", achc_rename::BASE_MODULE)
        }
        _ => false,
    }
}

impl Ds {
    fn new(p: &tast::Program, next: u32) -> Ds {
        let mut d = Ds {
            next,
            locals: HashMap::new(),
            tyvars: HashMap::new(),
            tuples: BTreeSet::new(),
            cons: HashMap::new(),
            datas: HashMap::new(),
            reached: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        let a = || Ty::Var("a".into());
        d.add_data(
            &list_tc(),
            vec!["a".into()],
            vec![(list_tc(), vec![]), (prim(":"), vec![a(), Ty::list(a())])],
            false,
        );
        d.add_data(&unit_tc(), vec![], vec![(unit_tc(), vec![])], false);
        for dd in &p.datas {
            d.add_data(
                &dd.name,
                dd.params.iter().map(|x| x.0.clone()).collect(),
                dd.cons
                    .iter()
                    .map(|k| (k.name.clone(), k.fields.clone()))
                    .collect(),
                dd.newtype,
            );
        }
        d
    }

    fn add_data(
        &mut self,
        name: &str,
        params: Vec<String>,
        cons: Vec<(String, Vec<Ty>)>,
        newtype: bool,
    ) {
        for (k, fs) in &cons {
            self.cons.insert(
                k.clone(),
                ConMeta {
                    data: name.to_string(),
                    fields: fs.clone(),
                },
            );
        }
        self.datas.insert(
            name.to_string(),
            DataMeta {
                params,
                cons: cons.into_iter().map(|x| x.0).collect(),
                newtype,
            },
        );
    }

    /// Constructor info, creating tuple types on demand.
    fn con(&mut self, n: &str) -> ConMeta {
        if let Some(k) = tuple_arity(n)
            && !self.cons.contains_key(n)
        {
            let ps: Vec<String> = (0..k).map(|i| format!("a{i}")).collect();
            let fs = ps.iter().map(|p| Ty::Var(p.clone())).collect();
            self.add_data(n, ps, vec![(n.to_string(), fs)], false);
        }
        if let Some(k) = tuple_arity(n) {
            self.tuples.insert(k);
        }
        match self.cons.get(n) {
            Some(m) => m.clone(),
            None => {
                self.ice(format!("unknown constructor `{n}`"));
                ConMeta {
                    data: String::new(),
                    fields: Vec::new(),
                }
            }
        }
    }

    fn ice(&mut self, m: String) {
        self.errors
            .push(Diagnostic::ice(format!("desugaring: {m}")));
    }

    fn id(&mut self, name: &str) -> c::Id {
        let i = c::Id::new(local_name(name), self.next);
        self.next += 1;
        i
    }

    fn bind_local(&mut self, name: &str) -> c::Id {
        let i = self.id(name);
        self.locals.insert(name.to_string(), i.clone());
        i
    }

    fn tv(&mut self, n: &str) -> c::TyVar {
        if let Some(v) = self.tyvars.get(n) {
            return v.clone();
        }
        let v = c::TyVar::new(achc_rename::display_name(n), self.next);
        self.next += 1;
        self.tyvars.insert(n.to_string(), v.clone());
        v
    }

    // ---- types ----

    fn ty(&mut self, t: &Ty) -> c::Type {
        match t {
            Ty::Var(v) => c::Type::Var(self.tv(v)),
            Ty::Meta(_) => c::Type::con("Unit"),
            Ty::Forall(vs, b) => {
                let body = self.ty(b);
                vs.iter().rev().fold(body, |acc, v| {
                    let k = Ds::kind(&kind_of(v, b));
                    c::Type::Forall(self.tv(v), k, Box::new(acc))
                })
            }
            Ty::Fun(a, b) => {
                let a = self.ty(a);
                let b = self.ty(b);
                c::Type::fun(c::Mult::Many, a, b)
            }
            Ty::Con(_) | Ty::App(..) => {
                let (h, args) = t.spine();
                let args: Vec<c::Type> = args.iter().map(|a| self.ty(a)).collect();
                match h {
                    Ty::Con(n) if *n == prim("->") && args.len() == 2 => {
                        c::Type::fun(c::Mult::Many, args[0].clone(), args[1].clone())
                    }
                    // LinearTypes: multiplicities are erased (ADR-037).
                    Ty::Con(n) if *n == prim("#FUN") && args.len() == 3 => {
                        c::Type::fun(c::Mult::Many, args[1].clone(), args[2].clone())
                    }
                    Ty::Con(n) if is_mult_con(n) => c::Type::con("Unit"),
                    Ty::Con(n) if *n == prim("Bytes") => c::Type::bytes(),
                    Ty::Con(n) => {
                        if let Some(k) = tuple_arity(n) {
                            self.tuples.insert(k);
                        }
                        c::Type::Con(core_tname(n), args)
                    }
                    _ => {
                        let h = self.ty(h);
                        c::Type::mk_app(h, args)
                    }
                }
            }
        }
    }

    fn kind(k: &Kind) -> c::Kind {
        match k {
            Kind::Star | Kind::Meta(_) => c::Kind::Type,
            Kind::Arrow(a, b) => c::Kind::Arrow(Box::new(Ds::kind(a)), Box::new(Ds::kind(b))),
        }
    }

    fn scheme(&mut self, tvs: &[(String, Kind)], t: &Ty) -> c::Type {
        let body = self.ty(t);
        tvs.iter().rev().fold(body, |b, (v, k)| {
            let v = self.tv(v);
            c::Type::Forall(v, Ds::kind(k), Box::new(b))
        })
    }

    fn tylams(&mut self, tvs: &[(String, Kind)], e: c::Expr) -> c::Expr {
        tvs.iter().rev().fold(e, |b, (v, k)| {
            let v = self.tv(v);
            c::Expr::TyLam(v, Ds::kind(k), Box::new(b))
        })
    }

    // ---- program ----

    fn program(&mut self, p: &tast::Program, prims: c::Program) -> c::Program {
        let mut out = c::Program {
            datas: prims.datas.clone(),
            externs: prims.externs.clone(),
            binds: Vec::new(),
            collect_cycles: false,
        };
        let builtin = |n: &str| {
            matches!(
                n,
                "Cirka.Prim.Int"
                    | "Cirka.Prim.Char"
                    | "Cirka.Prim.Double"
                    | "Cirka.Prim.Bytes"
                    | "Cirka.Prim.IO"
                    | "Cirka.Prim.IORef"
                    | "Cirka.Prim.Region"
                    | "Cirka.Prim.Array"
                    | "Cirka.Prim.ArrayPair"
                    | "Cirka.Prim.BytesPair"
                    | "Cirka.Prim.BigNat"
                    | "Cirka.Prim.Bool"
            )
        };
        for dd in &p.datas {
            if builtin(&dd.name) {
                continue;
            }
            let decl = self.data_decl(
                &core_tname(&dd.name),
                &dd.params,
                dd.cons
                    .iter()
                    .map(|k| (core_cname(&k.name), k.fields.clone(), k.strict.clone()))
                    .collect(),
                dd.newtype,
            );
            out.datas.push(decl);
        }
        // Primitives.
        let prim_tys: HashMap<String, c::Type> = prims
            .binds
            .iter()
            .map(|b| (b.name.clone(), b.ty.clone()))
            .collect();
        out.binds.extend(prims.binds);
        let datas: HashMap<&str, &tast::DataDecl> =
            p.datas.iter().map(|d| (d.name.as_str(), d)).collect();
        for f in &p.foreigns {
            if matches!(f.callconv.as_str(), "ccall" | "capi") {
                if let Err(d) = self.ccall(f, &datas, &mut out) {
                    self.errors.push(d);
                }
                continue;
            }
            let ty = self.scheme(&f.tvs, &f.ty);
            if f.callconv != "prim" {
                self.errors.push(
                    Diagnostic::error(format!(
                        "calling convention `{}` is not supported",
                        f.callconv
                    ))
                    .with_code("E0618")
                    .with_label(f.span, "")
                    .with_note("use `ccall` (or `prim` inside the base library)"),
                );
                continue;
            }
            let target = format!("prim${}", f.entity);
            match prim_tys.get(&target) {
                Some(t) if t.alpha_eq(&ty) => out.binds.push(c::TopBind {
                    name: f.name.clone(),
                    ty,
                    expr: c::Expr::Global(target),
                    span: f.span,
                }),
                // `IO` primitives are written against the representation.
                Some(t) if t.alpha_eq(&io_rep(&ty)) => out.binds.push(c::TopBind {
                    name: f.name.clone(),
                    ty: ty.clone(),
                    expr: cast_poly(c::Expr::Global(target), &ty),
                    span: f.span,
                }),
                Some(t) => self.errors.push(
                    Diagnostic::error(format!(
                        "the type of primitive `{}` does not match its declaration",
                        f.entity
                    ))
                    .with_code("E0607")
                    .with_label(f.span, "")
                    .with_note(format!(
                        "the primitive has Core type {t}, the declaration {ty}"
                    )),
                ),
                None => self.errors.push(
                    Diagnostic::error(format!("unknown primitive `{}`", f.entity))
                        .with_code("E0607")
                        .with_label(f.span, ""),
                ),
            }
        }
        for g in &p.binds {
            for b in g.binds() {
                let ty = self.scheme(&b.tvs, &b.ty);
                let e = self.expr(&b.rhs);
                let expr = self.tylams(&b.tvs, e);
                out.binds.push(c::TopBind {
                    name: b.name.clone(),
                    ty,
                    expr,
                    span: b.span,
                });
            }
        }
        if let Some(m) = &p.main {
            // `main : Int#` runs `Main.main` with the world token.
            let mt = p
                .binds
                .iter()
                .flat_map(|g| g.binds())
                .find(|b| &b.name == m)
                .map(|b| (b.tvs.len(), b.ty.clone()));
            if let Some((ntv, Ty::App(_, r))) = mt {
                let mut f = c::Expr::Global(m.clone());
                for _ in 0..ntv {
                    f = c::Expr::TyApp(Box::new(f), c::Type::con("Unit"));
                }
                let rt = if ntv > 0 {
                    c::Type::con("Unit")
                } else {
                    self.ty(&r)
                };
                let f = c::Expr::Cast(
                    Box::new(f),
                    c::Type::fun(c::Mult::Many, c::Type::unit(), rt.clone()),
                );
                let r = self.id("r");
                out.binds.push(c::TopBind {
                    name: "main".into(),
                    ty: c::Type::int(),
                    expr: c::Expr::LetStrict(
                        c::Binder::new(r, c::Mult::Many, rt),
                        Box::new(c::Expr::app(f, [c::Expr::Lit(c::Lit::Unit)])),
                        Box::new(c::Expr::int(0)),
                    ),
                    span: Span::DUMMY,
                });
            }
        }
        for k in std::mem::take(&mut self.tuples) {
            let ps: Vec<(String, Kind)> = (0..k).map(|i| (format!("a{i}"), Kind::Star)).collect();
            let fs = ps.iter().map(|p| Ty::Var(p.0.clone())).collect();
            let n = format!("Tuple{k}");
            let decl = self.data_decl(&n, &ps, vec![(n.clone(), fs, vec![false; k])], false);
            out.datas.push(decl);
        }
        out
    }

    fn data_decl(
        &mut self,
        name: &str,
        params: &[(String, Kind)],
        cons: Vec<(String, Vec<Ty>, Vec<bool>)>,
        newtype: bool,
    ) -> c::DataDecl {
        c::DataDecl {
            name: name.to_string(),
            params: params
                .iter()
                .map(|(v, k)| (self.tv(v), Ds::kind(k)))
                .collect(),
            linear: false,
            ctors: cons
                .into_iter()
                .map(|(n, fs, st)| c::CtorDecl {
                    name: n,
                    fields: fs.iter().map(|t| self.ty(t)).collect(),
                    strict: st,
                })
                .collect(),
            newtype,
        }
    }

    // ---- expressions ----

    fn expr(&mut self, e: &tast::Expr) -> c::Expr {
        match e {
            tast::Expr::Local(x, _) => match self.locals.get(x) {
                Some(i) => c::Expr::Var(i.clone()),
                None => {
                    self.ice(format!("unbound local `{x}`"));
                    c::Expr::Lit(c::Lit::Unit)
                }
            },
            tast::Expr::Global(g, inst) => {
                let head = match self.locals.get(g) {
                    Some(i) => c::Expr::Var(i.clone()),
                    None => c::Expr::Global(g.clone()),
                };
                inst.iter().fold(head, |f, t| {
                    let t = self.ty(t);
                    c::Expr::TyApp(Box::new(f), t)
                })
            }
            tast::Expr::Con(..) | tast::Expr::App(..) => {
                let mut args = Vec::new();
                let mut h = e;
                while let tast::Expr::App(f, a) = h {
                    args.push(&**a);
                    h = f;
                }
                args.reverse();
                if let tast::Expr::Con(k, inst) = h {
                    let args: Vec<c::Expr> = args.iter().map(|a| self.expr(a)).collect();
                    return self.con_app(k, inst, args);
                }
                let f = self.expr(h);
                let args: Vec<c::Expr> = args.iter().map(|a| self.expr(a)).collect();
                c::Expr::app(f, args)
            }
            tast::Expr::Lit(l) => self.lit(l),
            tast::Expr::Lam(x, t, b) => {
                let ty = self.ty(t);
                let i = self.bind_local(x);
                let b = self.expr(b);
                c::Expr::Lam(c::Binder::new(i, c::Mult::Many, ty), Box::new(b))
            }
            tast::Expr::Let(gs, b) => self.lets(gs, |s| s.expr(b)),
            tast::Expr::Match(vs, cls, t) => {
                let cols: Vec<Col> = vs
                    .iter()
                    .map(|(v, t)| match self.locals.get(v) {
                        Some(i) => (i.clone(), t.clone()),
                        None => {
                            self.ice(format!("unbound match variable `{v}`"));
                            (c::Id::new("v", 0), t.clone())
                        }
                    })
                    .collect();
                // Pattern variables are scoped to the match (a pattern
                // binding's selectors reuse the bound names).
                let saved = self.locals.clone();
                let e = self.top_match(cols, cls, t);
                self.locals = saved;
                e
            }
            tast::Expr::Error(m, t) => {
                let t = self.ty(t);
                c::Expr::Panic(m.clone(), t)
            }
            tast::Expr::Coerce(x, t) => {
                let x = self.expr(x);
                let t = self.ty(t);
                cast_poly(x, &t)
            }
            tast::Expr::Mark(_, x, _) => self.expr(x),
            tast::Expr::Hole(_) | tast::Expr::OverLit(..) => {
                self.ice("unresolved class dictionary".into());
                c::Expr::Lit(c::Lit::Unit)
            }
        }
    }

    fn con_app(&mut self, k: &str, inst: &[Ty], mut args: Vec<c::Expr>) -> c::Expr {
        let meta = self.con(k);
        let n = meta.fields.len();
        let tys: Vec<c::Type> = inst.iter().map(|t| self.ty(t)).collect();
        if args.len() > n {
            let rest = args.split_off(n);
            let f = self.con_app(k, inst, args);
            return c::Expr::app(f, rest);
        }
        // Eta-expand a partial application.
        let sub = self.data_subst(&meta.data, inst);
        let mut lams = Vec::new();
        for f in &meta.fields[args.len()..] {
            let t = f.subst(&sub);
            let ct = self.ty(&t);
            let i = self.id("x");
            args.push(c::Expr::Var(i.clone()));
            lams.push(c::Binder::new(i, c::Mult::Many, ct));
        }
        let body = c::Expr::Con(core_cname(k), tys, args);
        lams.into_iter()
            .rev()
            .fold(body, |b, x| c::Expr::Lam(x, Box::new(b)))
    }

    fn data_subst(&self, data: &str, inst: &[Ty]) -> Vec<(String, Ty)> {
        match self.datas.get(data) {
            Some(d) => d.params.iter().cloned().zip(inst.iter().cloned()).collect(),
            None => Vec::new(),
        }
    }

    fn lit(&mut self, l: &tast::Lit) -> c::Expr {
        match l {
            tast::Lit::Int(n) => c::Expr::Con("I#".into(), Vec::new(), vec![c::Expr::int(*n)]),
            // Resolved by the type checker; wraps like `fromInteger` to Int.
            tast::Lit::BigInt(n) => {
                c::Expr::Con("I#".into(), Vec::new(), vec![c::Expr::int(*n as i64)])
            }
            tast::Lit::Char(ch) => {
                c::Expr::Con("C#".into(), Vec::new(), vec![c::Expr::int(*ch as i64)])
            }
            tast::Lit::Double(x) => c::Expr::Con(
                "D#".into(),
                Vec::new(),
                vec![c::Expr::Lit(c::Lit::Double(*x))],
            ),
            tast::Lit::Str(s) => c::Expr::app(
                c::Expr::Global("prim$unpack".into()),
                [c::Expr::Lit(c::Lit::Str(s.as_bytes().to_vec()))],
            ),
        }
    }

    /// `let` bindings around `body`; strict (bang) bindings become `let!`.
    fn lets(&mut self, gs: &[BindGroup], body: impl FnOnce(&mut Self) -> c::Expr) -> c::Expr {
        let binds: Vec<(c::Bind, bool)> = gs
            .iter()
            .map(|g| {
                let strict = matches!(g, BindGroup::NonRec(b) if b.strict);
                (self.group(g), strict)
            })
            .collect();
        let body = body(self);
        binds
            .into_iter()
            .rev()
            .fold(body, |e, (b, strict)| match b {
                c::Bind::NonRec(x, r) if strict => c::Expr::LetStrict(x, r, Box::new(e)),
                b => c::Expr::Let(b, Box::new(e)),
            })
    }

    fn group(&mut self, g: &BindGroup) -> c::Bind {
        // Bind the names first: a recursive group refers to itself.
        let ids: Vec<c::Id> = g.binds().iter().map(|b| self.bind_local(&b.name)).collect();
        let mut out = Vec::new();
        for (b, i) in g.binds().iter().zip(ids) {
            let ty = self.scheme(&b.tvs, &b.ty);
            let e = self.expr(&b.rhs);
            let e = self.tylams(&b.tvs, e);
            out.push((c::Binder::new(i, c::Mult::Many, ty), e));
        }
        match g {
            BindGroup::NonRec(_) => {
                let (b, e) = out.pop().unwrap();
                c::Bind::NonRec(b, Box::new(e))
            }
            BindGroup::Rec(bs) => {
                // A value (not a function) that refers to its own group
                // makes a cycle (knot-tying, 04-memory-model.md §5). A
                // function-typed binding defined without arguments (`f = g
                // where g x = ..`) becomes a function once optimized.
                for ((_, e), b) in out.iter().zip(bs) {
                    if !matches!(strip_tylams(e), c::Expr::Lam(..))
                        && !is_fun_type(&b.ty)
                        && !b.span.is_dummy()
                    {
                        self.warnings.push(
                            Diagnostic::warning(format!(
                                "the recursive value binding `{}` may form a cycle",
                                display_name(&b.name)
                            ))
                            .with_code("W0401")
                            .with_label(b.span, "refers to itself (through its group)")
                            .with_note(
                                "reference counting cannot free a cycle: it leaks \
                                 unless the program is built with `--cycles=collect`",
                            )
                            .with_help(
                                "build it with `--cycles=collect`, or write it without \
                                 sharing (e.g. a recursive function)",
                            ),
                        );
                    }
                }
                c::Bind::Rec(out)
            }
        }
    }

    // ---- pattern matching ----

    fn top_match(&mut self, cols: Vec<Col>, cls: &[Clause], t: &Ty) -> c::Expr {
        let res = self.ty(t);
        let saved = std::mem::replace(&mut self.reached, vec![false; cls.len()]);
        let rows: Vec<Row> = cls
            .iter()
            .enumerate()
            .map(|(i, cl)| Row {
                pats: cl.pats.clone(),
                rhs: RowRhs::Rhs(&cl.rhs),
                lets: Vec::new(),
                clause: Some(i),
            })
            .collect();
        let fail = self.id("fail");
        let e = self.matcher(&cols, rows, &c::Expr::Var(fail.clone()), &res);
        let span = cls.first().map(|c| c.span).unwrap_or(Span::DUMMY);
        let e = if count_var(&e, &fail) > 0 {
            // The decision tree may keep failure paths that are
            // impossible; warn only when some value is really unmatched.
            if !span.is_dummy() && self.non_exhaustive(cls) {
                self.warnings.push(
                    Diagnostic::warning("non-exhaustive patterns")
                        .with_code("W0610")
                        .with_label(span, "")
                        .with_note("a value not matched by any equation is a runtime error"),
                );
            }
            let msg = "non-exhaustive patterns".to_string();
            subst_var(e, &fail, &c::Expr::Panic(msg, res.clone()))
        } else {
            e
        };
        for (i, r) in self.reached.iter().enumerate() {
            if !r && !cls[i].span.is_dummy() {
                self.warnings.push(
                    Diagnostic::warning("this equation is redundant")
                        .with_code("W0611")
                        .with_label(cls[i].span, "")
                        .with_note("earlier equations match every value it matches"),
                );
            }
        }
        self.reached = saved;
        e
    }

    /// `first(fail')` where `fail'` runs `rest(fail)`; `rest` is only
    /// generated when `first` can fail.
    fn with_fail(
        &mut self,
        res: &c::Type,
        fail: &c::Expr,
        first: impl FnOnce(&mut Self, &c::Expr) -> c::Expr,
        rest: impl FnOnce(&mut Self, &c::Expr) -> c::Expr,
    ) -> c::Expr {
        let k = self.id("fail");
        let e = first(self, &c::Expr::Var(k.clone()));
        match count_var(&e, &k) {
            0 => e,
            n => {
                let r = rest(self, fail);
                if n == 1
                    || r.is_atom()
                    || matches!(&r, c::Expr::Panic(..))
                    || matches!(&r, c::Expr::Con(_, _, a) if a.is_empty())
                {
                    subst_var(e, &k, &r)
                } else {
                    c::Expr::Let(
                        c::Bind::NonRec(c::Binder::new(k, c::Mult::Many, res.clone()), Box::new(r)),
                        Box::new(e),
                    )
                }
            }
        }
    }

    fn matcher(
        &mut self,
        cols: &[Col],
        mut rows: Vec<Row<'_>>,
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        if rows.is_empty() {
            return fail.clone();
        }
        if cols.is_empty() {
            let first = rows.remove(0);
            return self.with_fail(
                res,
                fail,
                |s, f| s.row_rhs(first, f, res),
                |s, f| s.matcher(cols, rows, f, res),
            );
        }
        // Bind variables and normalize the first column.
        for r in rows.iter_mut() {
            let p = r.pats[0].clone();
            r.pats[0] = self.normalize(p, &cols[0], &mut r.lets);
        }
        let class = |p: &Pat| match p {
            Pat::Wild(_) => 0,
            Pat::Con(..) => 1,
            Pat::Lit(tast::Lit::Int(_) | tast::Lit::Char(_), _) => 2,
            _ => 3,
        };
        let k = class(&rows[0].pats[0]);
        let n = if k == 3 {
            1
        } else {
            rows.iter().take_while(|r| class(&r.pats[0]) == k).count()
        };
        let rest: Vec<Row> = rows.split_off(n);
        let block = rows;
        self.with_fail(
            res,
            fail,
            |s, f| match k {
                0 => {
                    let rows = block
                        .into_iter()
                        .map(|mut r| {
                            r.pats.remove(0);
                            r
                        })
                        .collect();
                    s.matcher(&cols[1..], rows, f, res)
                }
                1 => s.con_block(cols, block, f, res),
                2 => s.lit_block(cols, block, f, res),
                _ => s.special(cols, block.into_iter().next().unwrap(), f, res),
            },
            |s, f| s.matcher(cols, rest, f, res),
        )
    }

    /// Binds variables of the pattern to the column and strips them:
    /// `x` and `x@p` bind `x`; lazy patterns bind their variables lazily.
    fn normalize(&mut self, p: Pat, col: &Col, lets: &mut Vec<(c::Binder, c::Expr)>) -> Pat {
        match p {
            Pat::Var(x, t) => {
                self.locals.insert(x, col.0.clone());
                Pat::Wild(t)
            }
            Pat::As(x, _, q) => {
                self.locals.insert(x, col.0.clone());
                self.normalize(*q, col, lets)
            }
            Pat::Lazy(q) => {
                if let Pat::Wild(_) | Pat::Var(..) = *q {
                    return self.normalize(*q, col, lets);
                }
                let mut vars = Vec::new();
                pat_vars(&q, &mut vars);
                let mut bound = Vec::new();
                for (v, vt) in vars {
                    // `v = case col of q -> v; _ -> error`
                    let cvt = self.ty(&vt);
                    let saved = std::mem::take(&mut self.reached);
                    let inner = self.matcher(
                        std::slice::from_ref(col),
                        vec![Row {
                            pats: vec![(*q).clone()],
                            rhs: RowRhs::Expr(c::Expr::Lit(c::Lit::Unit)),
                            lets: Vec::new(),
                            clause: None,
                        }],
                        &c::Expr::Panic("irrefutable pattern failed".into(), cvt.clone()),
                        &cvt,
                    );
                    self.reached = saved;
                    // The matcher bound `v` to its field variable.
                    let val = match self.locals.get(&v) {
                        Some(i) => c::Expr::Var(i.clone()),
                        None => c::Expr::Panic("irrefutable pattern".into(), cvt.clone()),
                    };
                    let body = replace_unit_leaf(inner, &val);
                    let i = self.id(&v);
                    bound.push((v, i.clone()));
                    lets.push((c::Binder::new(i, c::Mult::Many, cvt), body));
                }
                // Each selector re-binds all the pattern's variables to its
                // own fields; the variables finally refer to the lets.
                for (v, i) in bound {
                    self.locals.insert(v, i);
                }
                Pat::Wild(col.1.clone())
            }
            Pat::Lit(tast::Lit::Str(s), _) => {
                // A string pattern is a list of characters.
                let ch = Ty::Con(prim("Char"));
                let nil = Pat::Con(list_tc(), vec![ch.clone()], Vec::new());
                s.chars().rev().fold(nil, |acc, x| {
                    Pat::Con(
                        prim(":"),
                        vec![ch.clone()],
                        vec![Pat::Lit(tast::Lit::Char(x), ch.clone()), acc],
                    )
                })
            }
            p => p,
        }
    }

    fn con_block(
        &mut self,
        cols: &[Col],
        rows: Vec<Row<'_>>,
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        let (v, vt) = cols[0].clone();
        let mut order: Vec<String> = Vec::new();
        let mut inst: Vec<Ty> = Vec::new();
        for r in &rows {
            if let Pat::Con(k, i, _) = &r.pats[0]
                && !order.contains(k)
            {
                order.push(k.clone());
                inst = i.clone();
            }
        }
        let data = self.con(&order[0]).data;
        let all: Vec<String> = self
            .datas
            .get(&data)
            .map(|d| d.cons.clone())
            .unwrap_or_default();
        let sub = self.data_subst(&data, &inst);
        // Matching a newtype constructor evaluates nothing (Haskell 2010
        // §4.2.3): the field is the column at another type, and the
        // patterns of the field decide what gets evaluated (BUG-0008).
        if self.datas.get(&data).is_some_and(|d| d.newtype) {
            let ft = self.con(&order[0]).fields[0].subst(&sub);
            let cft = self.ty(&ft);
            if !cft.is_unboxed() {
                let f = self.id("f");
                let mut ncols = vec![(f.clone(), ft)];
                ncols.extend(cols[1..].iter().cloned());
                let sub_rows = rows.iter().map(open_con).collect();
                let rhs = self.matcher(&ncols, sub_rows, fail, res);
                return c::Expr::Let(
                    c::Bind::NonRec(
                        c::Binder::new(f, c::Mult::Many, cft.clone()),
                        Box::new(c::Expr::Cast(Box::new(c::Expr::Var(v)), cft)),
                    ),
                    Box::new(rhs),
                );
            }
        }
        let mut alts = Vec::new();
        for k in &order {
            let meta = self.con(k);
            let fcols: Vec<Col> = meta
                .fields
                .iter()
                .map(|f| (self.id("f"), f.subst(&sub)))
                .collect();
            let sub_rows: Vec<Row> = rows
                .iter()
                .filter(|r| matches!(&r.pats[0], Pat::Con(x, _, _) if x == k))
                .map(open_con)
                .collect();
            let mut ncols = fcols.clone();
            ncols.extend(cols[1..].iter().cloned());
            let rhs = self.matcher(&ncols, sub_rows, fail, res);
            let binders = fcols
                .iter()
                .map(|(i, t)| {
                    let t = self.ty(t);
                    c::Binder::new(i.clone(), c::Mult::Many, t)
                })
                .collect();
            alts.push(c::Alt {
                con: c::AltCon::Con(core_cname(k), binders),
                rhs,
            });
        }
        if all.iter().any(|k| !order.contains(k)) {
            alts.push(c::Alt {
                con: c::AltCon::Default,
                rhs: fail.clone(),
            });
        }
        let st = self.ty(&vt);
        let b = self.id("s");
        c::Expr::Case(
            Box::new(c::Expr::Var(v)),
            c::Binder::new(b, c::Mult::Many, st),
            res.clone(),
            alts,
        )
    }

    /// Integer and character literals: unbox once and switch.
    fn lit_block(
        &mut self,
        cols: &[Col],
        rows: Vec<Row<'_>>,
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        let (v, vt) = cols[0].clone();
        let key = |p: &Pat| match p {
            Pat::Lit(tast::Lit::Int(n), _) => *n,
            Pat::Lit(tast::Lit::Char(ch), _) => *ch as i64,
            _ => 0,
        };
        let is_char = matches!(rows[0].pats[0], Pat::Lit(tast::Lit::Char(_), _));
        let mut order: Vec<i64> = Vec::new();
        for r in &rows {
            let n = key(&r.pats[0]);
            if !order.contains(&n) {
                order.push(n);
            }
        }
        let mut alts = Vec::new();
        for n in order {
            let sub_rows: Vec<Row> = rows
                .iter()
                .filter(|r| key(&r.pats[0]) == n)
                .map(|r| {
                    let mut r = r.clone();
                    r.pats.remove(0);
                    r
                })
                .collect();
            let rhs = self.matcher(&cols[1..], sub_rows, fail, res);
            alts.push(c::Alt {
                con: c::AltCon::Int(n),
                rhs,
            });
        }
        alts.push(c::Alt {
            con: c::AltCon::Default,
            rhs: fail.clone(),
        });
        let u = self.id("u");
        let inner = c::Expr::Case(
            Box::new(c::Expr::Var(u.clone())),
            c::Binder::new(self.id("k"), c::Mult::Many, c::Type::int()),
            res.clone(),
            alts,
        );
        let st = self.ty(&vt);
        let b = self.id("s");
        c::Expr::Case(
            Box::new(c::Expr::Var(v)),
            c::Binder::new(b, c::Mult::Many, st),
            res.clone(),
            vec![c::Alt {
                con: c::AltCon::Con(
                    if is_char { "C#" } else { "I#" }.into(),
                    vec![c::Binder::new(u, c::Mult::Many, c::Type::int())],
                ),
                rhs: inner,
            }],
        )
    }

    /// Bang patterns and floating-point literals (one row).
    fn special(
        &mut self,
        cols: &[Col],
        mut row: Row<'_>,
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        let (v, vt) = cols[0].clone();
        let st = self.ty(&vt);
        match row.pats[0].clone() {
            Pat::Bang(q) => {
                row.pats[0] = *q;
                let rest = self.matcher(cols, vec![row], fail, res);
                let b = self.id("s");
                c::Expr::Case(
                    Box::new(c::Expr::Var(v)),
                    c::Binder::new(b, c::Mult::Many, st),
                    res.clone(),
                    vec![c::Alt {
                        con: c::AltCon::Default,
                        rhs: rest,
                    }],
                )
            }
            Pat::Lit(tast::Lit::Double(x), _) => {
                row.pats.remove(0);
                let rest = self.matcher(&cols[1..], vec![row], fail, res);
                let u = self.id("u");
                let test = c::Expr::Case(
                    Box::new(c::Expr::Prim(
                        c::PrimOp::FEq,
                        Vec::new(),
                        vec![c::Expr::Var(u.clone()), c::Expr::Lit(c::Lit::Double(x))],
                    )),
                    c::Binder::new(self.id("k"), c::Mult::Many, c::Type::int()),
                    res.clone(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Int(0),
                            rhs: fail.clone(),
                        },
                        c::Alt {
                            con: c::AltCon::Default,
                            rhs: rest,
                        },
                    ],
                );
                let b = self.id("s");
                c::Expr::Case(
                    Box::new(c::Expr::Var(v)),
                    c::Binder::new(b, c::Mult::Many, st),
                    res.clone(),
                    vec![c::Alt {
                        con: c::AltCon::Con(
                            "D#".into(),
                            vec![c::Binder::new(u, c::Mult::Many, c::Type::double())],
                        ),
                        rhs: test,
                    }],
                )
            }
            Pat::OverLit(_, test, _) => {
                row.pats.remove(0);
                let rest = self.matcher(&cols[1..], vec![row], fail, res);
                let t = self.expr(&test);
                let call = c::Expr::app(t, [c::Expr::Var(v)]);
                c::Expr::Case(
                    Box::new(call),
                    c::Binder::new(self.id("k"), c::Mult::Many, c::Type::con("Bool")),
                    res.clone(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Con("True".into(), Vec::new()),
                            rhs: rest,
                        },
                        c::Alt {
                            con: c::AltCon::Default,
                            rhs: fail.clone(),
                        },
                    ],
                )
            }
            p => {
                self.ice(format!("unexpected pattern {}", tast::pat(&p)));
                fail.clone()
            }
        }
    }

    fn pat_ty(&mut self, p: &Pat) -> Ty {
        match p {
            Pat::Var(_, t)
            | Pat::Wild(t)
            | Pat::Lit(_, t)
            | Pat::As(_, t, _)
            | Pat::OverLit(_, _, t) => t.clone(),
            Pat::Con(k, inst, _) => {
                let data = self.con(k).data;
                Ty::con(&data, inst.clone())
            }
            Pat::Lazy(q) | Pat::Bang(q) => self.pat_ty(q),
        }
    }

    // ---- right-hand sides and guards ----

    fn row_rhs(&mut self, row: Row<'_>, fail: &c::Expr, res: &c::Type) -> c::Expr {
        if let Some(i) = row.clause
            && let Some(r) = self.reached.get_mut(i)
        {
            *r = true;
        }
        let body = match row.rhs {
            RowRhs::Expr(e) => e,
            RowRhs::Guards(gs, e) => self.guards(gs, e, fail, res),
            RowRhs::Rhs(r) => self.lets(&r.wheres, |s| s.alternatives(&r.body, fail, res)),
        };
        row.lets.into_iter().rev().fold(body, |e, (b, r)| {
            c::Expr::Let(c::Bind::NonRec(b, Box::new(r)), Box::new(e))
        })
    }

    fn alternatives(
        &mut self,
        alts: &[(Vec<Guard>, tast::Expr)],
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        let Some(((gs, e), rest)) = alts.split_first() else {
            return fail.clone();
        };
        self.with_fail(
            res,
            fail,
            |s, f| s.guards(gs, e, f, res),
            |s, f| s.alternatives(rest, f, res),
        )
    }

    fn guards(
        &mut self,
        gs: &[Guard],
        body: &tast::Expr,
        fail: &c::Expr,
        res: &c::Type,
    ) -> c::Expr {
        let Some((g, rest)) = gs.split_first() else {
            return self.expr(body);
        };
        match g {
            Guard::Bool(b) if is_true(b) => self.guards(rest, body, fail, res),
            Guard::Bool(b) => {
                let cond = self.expr(b);
                let then = self.guards(rest, body, fail, res);
                c::Expr::Case(
                    Box::new(cond),
                    c::Binder::new(self.id("g"), c::Mult::Many, c::Type::con("Bool")),
                    res.clone(),
                    vec![
                        c::Alt {
                            con: c::AltCon::Con("True".into(), Vec::new()),
                            rhs: then,
                        },
                        c::Alt {
                            con: c::AltCon::Default,
                            rhs: fail.clone(),
                        },
                    ],
                )
            }
            Guard::Let(bs) => self.lets(bs, |s| s.guards(rest, body, fail, res)),
            Guard::Bind(p, e) => {
                let t = self.pat_ty(p);
                let ct = self.ty(&t);
                let x = self.id("pg");
                let rhs = self.expr(e);
                let saved = std::mem::take(&mut self.reached);
                let m = self.matcher(
                    &[(x.clone(), t)],
                    vec![Row {
                        pats: vec![p.clone()],
                        rhs: RowRhs::Guards(rest, body),
                        lets: Vec::new(),
                        clause: None,
                    }],
                    fail,
                    res,
                );
                self.reached = saved;
                c::Expr::Let(
                    c::Bind::NonRec(c::Binder::new(x, c::Mult::Many, ct), Box::new(rhs)),
                    Box::new(m),
                )
            }
        }
    }
}

/// `r` with the field patterns of its first column in front of the rest.
fn open_con<'a>(r: &Row<'a>) -> Row<'a> {
    let mut r = r.clone();
    let Pat::Con(_, _, ps) = r.pats.remove(0) else {
        unreachable!()
    };
    let mut pats = ps;
    pats.extend(r.pats);
    r.pats = pats;
    r
}

fn pat_vars(p: &Pat, out: &mut Vec<(String, Ty)>) {
    match p {
        Pat::Var(v, t) => out.push((v.clone(), t.clone())),
        Pat::As(v, t, q) => {
            out.push((v.clone(), t.clone()));
            pat_vars(q, out);
        }
        Pat::Con(_, _, ps) => ps.iter().for_each(|q| pat_vars(q, out)),
        Pat::Lazy(q) | Pat::Bang(q) => pat_vars(q, out),
        _ => {}
    }
}

/// Replaces the `unit` leaves produced for an irrefutable pattern's
/// selector by the selected variable.
fn replace_unit_leaf(e: c::Expr, val: &c::Expr) -> c::Expr {
    map_expr(e, &mut |x| match x {
        c::Expr::Lit(c::Lit::Unit) => Ok(val.clone()),
        x => Err(x),
    })
}

fn count_var(e: &c::Expr, v: &c::Id) -> usize {
    let mut n = 0;
    visit(e, &mut |x| {
        if let c::Expr::Var(i) = x
            && i == v
        {
            n += 1;
        }
    });
    n
}

fn subst_var(e: c::Expr, v: &c::Id, r: &c::Expr) -> c::Expr {
    map_expr(e, &mut |x| match x {
        c::Expr::Var(i) if &i == v => Ok(r.clone()),
        x => Err(x),
    })
}

fn visit(e: &c::Expr, f: &mut dyn FnMut(&c::Expr)) {
    f(e);
    match e {
        c::Expr::Var(_) | c::Expr::Global(_) | c::Expr::Lit(_) | c::Expr::Panic(..) => {}
        c::Expr::Con(_, _, a) | c::Expr::Prim(_, _, a) | c::Expr::Extern(_, a) => {
            a.iter().for_each(|x| visit(x, f))
        }
        c::Expr::App(a, b) => {
            visit(a, f);
            visit(b, f);
        }
        c::Expr::TyApp(x, _)
        | c::Expr::TyLam(_, _, x)
        | c::Expr::Cast(x, _)
        | c::Expr::Lam(_, x) => visit(x, f),
        c::Expr::Let(b, x) => {
            match b {
                c::Bind::NonRec(_, r) => visit(r, f),
                c::Bind::Rec(bs) => bs.iter().for_each(|(_, r)| visit(r, f)),
            }
            visit(x, f);
        }
        c::Expr::LetStrict(_, r, x) => {
            visit(r, f);
            visit(x, f);
        }
        c::Expr::Case(s, _, _, alts) => {
            visit(s, f);
            alts.iter().for_each(|a| visit(&a.rhs, f));
        }
    }
}

/// Rebuilds `e` top-down: `f` returns `Ok(replacement)` or gives the
/// node back to be traversed.
fn map_expr(e: c::Expr, f: &mut dyn FnMut(c::Expr) -> Result<c::Expr, c::Expr>) -> c::Expr {
    let e = match f(e) {
        Ok(r) => return r,
        Err(e) => e,
    };
    let mut go = |x: Box<c::Expr>| Box::new(map_expr(*x, f));
    match e {
        c::Expr::Var(_) | c::Expr::Global(_) | c::Expr::Lit(_) | c::Expr::Panic(..) => e,
        c::Expr::Con(k, t, a) => {
            c::Expr::Con(k, t, a.into_iter().map(|x| *go(Box::new(x))).collect())
        }
        c::Expr::Prim(o, t, a) => {
            c::Expr::Prim(o, t, a.into_iter().map(|x| *go(Box::new(x))).collect())
        }
        c::Expr::Extern(n, a) => {
            c::Expr::Extern(n, a.into_iter().map(|x| *go(Box::new(x))).collect())
        }
        c::Expr::App(a, b) => {
            let a = go(a);
            c::Expr::App(a, go(b))
        }
        c::Expr::TyApp(x, t) => c::Expr::TyApp(go(x), t),
        c::Expr::TyLam(v, k, x) => c::Expr::TyLam(v, k, go(x)),
        c::Expr::Cast(x, t) => c::Expr::Cast(go(x), t),
        c::Expr::Lam(b, x) => c::Expr::Lam(b, go(x)),
        c::Expr::Let(b, x) => {
            let b = match b {
                c::Bind::NonRec(v, r) => c::Bind::NonRec(v, go(r)),
                c::Bind::Rec(bs) => {
                    c::Bind::Rec(bs.into_iter().map(|(v, r)| (v, *go(Box::new(r)))).collect())
                }
            };
            c::Expr::Let(b, go(x))
        }
        c::Expr::LetStrict(v, r, x) => {
            let r = go(r);
            c::Expr::LetStrict(v, r, go(x))
        }
        c::Expr::Case(s, b, t, alts) => {
            let s = go(s);
            c::Expr::Case(
                s,
                b,
                t,
                alts.into_iter()
                    .map(|a| c::Alt {
                        con: a.con,
                        rhs: *go(Box::new(a.rhs)),
                    })
                    .collect(),
            )
        }
    }
}

/// Removes top-level `prim$` definitions and externs nothing refers to.
fn prune(p: &mut c::Program) {
    let mut live: BTreeSet<String> = p
        .binds
        .iter()
        .filter(|b| !b.name.starts_with("prim$"))
        .map(|b| b.name.clone())
        .collect();
    let mut externs = BTreeSet::new();
    let mut work: Vec<String> = live.iter().cloned().collect();
    while let Some(n) = work.pop() {
        let Some(b) = p.binds.iter().find(|b| b.name == n) else {
            continue;
        };
        visit(&b.expr, &mut |x| match x {
            c::Expr::Global(g) => {
                if live.insert(g.clone()) {
                    work.push(g.clone());
                }
            }
            c::Expr::Extern(e, _) => {
                externs.insert(e.clone());
            }
            _ => {}
        });
    }
    p.binds.retain(|b| live.contains(&b.name));
    p.externs.retain(|e| externs.contains(&e.name));
}

/// `e` without its type abstractions.
fn strip_tylams(mut e: &c::Expr) -> &c::Expr {
    while let c::Expr::TyLam(_, _, b) = e {
        e = b;
    }
    e
}

/// A local name as written (without the renamer's `#n` suffix).
fn display_name(n: &str) -> &str {
    n.split('#').next().unwrap_or(n)
}

/// Whether `t` is a function type (with any multiplicity).
fn is_fun_type(t: &achc_typeck::types::Ty) -> bool {
    use achc_typeck::types::{Ty, fun_tc};
    match t {
        Ty::Fun(..) => true,
        Ty::Forall(_, b) => is_fun_type(b),
        Ty::App(f, _) => {
            matches!(&**f, Ty::App(g, _) if matches!(&**g, Ty::App(h, _) if matches!(&**h, Ty::Con(c) if *c == fun_tc())))
        }
        _ => false,
    }
}
