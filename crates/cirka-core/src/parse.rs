//! Parser of the Core text format (S-expressions; see 06-ir.md §1.1).
//!
//! ```text
//! (data List (a) (Nil) (Cons a (List a)))
//! (extern cirka_write_i64 (Int#) Unit#)
//! (def map (forall (a b) (-> (-> a b) (List a) (List b)))
//!   (tlam (a b) (lam ((f (-> a b)) (xs (List a)))
//!     (case xs (s (List a)) (List b)
//!       ((Nil) (con Nil (b)))
//!       ((Cons y ys) (con Cons (b) (f y) (map @ ...)))))))
//! ```

use crate::expr::*;
use crate::sexp::{Sexp, read};
use crate::ty::*;
use cirka_diagnostics::{Diagnostic, FileId, Span};
use std::collections::HashSet;

type R<T> = Result<T, Diagnostic>;

fn err<T>(span: Span, msg: impl Into<String>) -> R<T> {
    Err(Diagnostic::error(msg.into())
        .with_code("C0001")
        .with_label(span, "here"))
}

struct Parser {
    datas: Vec<DataDecl>,
    globals: HashSet<String>,
    vars: Vec<(String, Id, Type)>,
    tyvars: Vec<TyVar>,
    next: u32,
}

/// Parses a Core program.
pub fn parse_program(file: FileId, src: &str) -> R<Program> {
    let forms = read(file, src)?;
    let mut p = Parser {
        datas: Vec::new(),
        globals: HashSet::new(),
        vars: Vec::new(),
        tyvars: Vec::new(),
        next: 1,
    };
    // Pass 1: names of data types / globals (forward references).
    for f in &forms {
        match (f.head(), f.list()) {
            (Some("def"), Some([_, Sexp::Sym(n, _), ..])) => {
                p.globals.insert(n.clone());
            }
            (Some("data"), Some([_, Sexp::Sym(n, _), ..])) => p.datas.push(DataDecl {
                name: n.clone(),
                params: vec![],
                linear: false,
                ctors: vec![],
                newtype: false,
            }),
            _ => {}
        }
    }
    let mut prog = Program::default();
    // Data types first, so that patterns may refer to types declared later.
    for f in &forms {
        if f.head() == Some("data") {
            let d = p.data(f)?;
            if let Some(slot) = p.datas.iter_mut().find(|x| x.name == d.name) {
                *slot = d.clone();
            }
            prog.datas.push(d);
        }
    }
    for f in &forms {
        match f.head() {
            Some("data") => {}
            Some("extern") => prog.externs.push(p.extern_decl(f)?),
            Some("def") => prog.binds.push(p.def(f)?),
            _ => return err(f.span(), "expected `data`, `extern` or `def`"),
        }
    }
    Ok(prog)
}

/// Parses a single type (for tests and tools).
pub fn parse_type(file: FileId, src: &str) -> R<Type> {
    let forms = read(file, src)?;
    let mut p = Parser {
        datas: Vec::new(),
        globals: HashSet::new(),
        vars: Vec::new(),
        tyvars: Vec::new(),
        next: 1,
    };
    match forms.as_slice() {
        [t] => p.ty(t),
        _ => err(Span::new(file, 0, 0), "expected one type"),
    }
}

fn mult_of(s: &Sexp) -> Option<Mult> {
    match s {
        Sexp::Int(0, _) => Some(Mult::Zero),
        Sexp::Int(1, _) => Some(Mult::One),
        Sexp::Sym(w, _) if w == "ω" || w == "w" => Some(Mult::Many),
        _ => None,
    }
}

impl Parser {
    fn fresh(&mut self, name: &str) -> Id {
        let id = Id::new(name, self.next);
        self.next += 1;
        id
    }

    fn fresh_tv(&mut self, name: &str) -> TyVar {
        let v = TyVar::new(name, self.next);
        self.next += 1;
        v
    }

    fn sym<'a>(&self, s: &'a Sexp, what: &str) -> R<&'a str> {
        match s {
            Sexp::Sym(x, _) => Ok(x),
            _ => err(s.span(), format!("expected {what}")),
        }
    }

    fn list<'a>(&self, s: &'a Sexp, what: &str) -> R<&'a [Sexp]> {
        match s {
            Sexp::List(l, _) => Ok(l),
            _ => err(s.span(), format!("expected {what}")),
        }
    }

    fn kind(&self, s: &Sexp) -> R<Kind> {
        match s {
            Sexp::Sym(k, _) if k == "Type" => Ok(Kind::Type),
            Sexp::Sym(k, _) if k == "LType" => Ok(Kind::LType),
            Sexp::Sym(k, _) if k == "#" => Ok(Kind::Unboxed),
            Sexp::List(l, _) if l.len() >= 3 && l[0].sym() == Some("->") => {
                let mut k = self.kind(&l[l.len() - 1])?;
                for a in l[1..l.len() - 1].iter().rev() {
                    k = Kind::Arrow(Box::new(self.kind(a)?), Box::new(k));
                }
                Ok(k)
            }
            _ => err(s.span(), "expected a kind (Type, LType, #, (-> k k))"),
        }
    }

    /// `a` or `(a Kind)`.
    fn tv_binder(&mut self, s: &Sexp) -> R<(TyVar, Kind)> {
        match s {
            Sexp::Sym(a, _) => Ok((self.fresh_tv(a), Kind::Type)),
            Sexp::List(l, _) if l.len() == 2 => {
                let a = self.sym(&l[0], "a type variable")?.to_string();
                let k = self.kind(&l[1])?;
                Ok((self.fresh_tv(&a), k))
            }
            _ => err(
                s.span(),
                "expected a type variable binder `a` or `(a Kind)`",
            ),
        }
    }

    fn ty(&mut self, s: &Sexp) -> R<Type> {
        match s {
            Sexp::Sym(x, _) => {
                if x == "Any" {
                    return Ok(Type::Any);
                }
                if let Some(v) = self.tyvars.iter().rev().find(|v| &v.name == x) {
                    return Ok(Type::Var(v.clone()));
                }
                Ok(Type::Con(x.clone(), vec![]))
            }
            Sexp::List(l, sp) => {
                let Some(head) = l.first() else {
                    return err(*sp, "empty type");
                };
                match head.sym() {
                    Some("->") => {
                        let mut rest = &l[1..];
                        let mut m = Mult::Many;
                        if let Some(x) = rest.first().and_then(mult_of) {
                            m = x;
                            rest = &rest[1..];
                        }
                        if rest.len() < 2 {
                            return err(*sp, "`->` needs an argument and a result type");
                        }
                        let mut t = self.ty(&rest[rest.len() - 1])?;
                        for a in rest[..rest.len() - 1].iter().rev() {
                            t = Type::fun(m, self.ty(a)?, t);
                        }
                        Ok(t)
                    }
                    Some("forall") => {
                        if l.len() != 3 {
                            return err(*sp, "expected (forall (binders) type)");
                        }
                        let bs = self.list(&l[1], "type variable binders")?;
                        let n = self.tyvars.len();
                        let mut vs = Vec::new();
                        for b in bs {
                            let (v, k) = self.tv_binder(b)?;
                            self.tyvars.push(v.clone());
                            vs.push((v, k));
                        }
                        let body = self.ty(&l[2]);
                        self.tyvars.truncate(n);
                        let mut t = body?;
                        for (v, k) in vs.into_iter().rev() {
                            t = Type::Forall(v, k, Box::new(t));
                        }
                        Ok(t)
                    }
                    Some(c) => {
                        let mut args = Vec::new();
                        for a in &l[1..] {
                            args.push(self.ty(a)?);
                        }
                        if let Some(v) = self.tyvars.iter().rev().find(|v| v.name == c) {
                            return Ok(Type::mk_app(Type::Var(v.clone()), args));
                        }
                        Ok(Type::Con(c.to_string(), args))
                    }
                    None => err(head.span(), "expected a type constructor"),
                }
            }
            _ => err(s.span(), "expected a type"),
        }
    }

    fn data(&mut self, f: &Sexp) -> R<DataDecl> {
        let l = f.list().unwrap();
        if l.len() < 3 {
            return err(f.span(), "expected (data Name (params) ctors...)");
        }
        let name = self.sym(&l[1], "a data type name")?.to_string();
        let n = self.tyvars.len();
        let mut params = Vec::new();
        for b in self.list(&l[2], "type parameters")? {
            let (v, k) = self.tv_binder(b)?;
            self.tyvars.push(v.clone());
            params.push((v, k));
        }
        let mut d = DataDecl {
            name,
            params,
            linear: false,
            ctors: vec![],
            newtype: false,
        };
        let r = (|| {
            for c in &l[3..] {
                match c {
                    Sexp::Sym(s, _) if s == ":linear" => d.linear = true,
                    Sexp::Sym(s, _) if s == ":newtype" => d.newtype = true,
                    Sexp::List(cl, sp) => {
                        let Some(cn) = cl.first().and_then(|x| x.sym()) else {
                            return err(*sp, "expected (Ctor fields...)");
                        };
                        let mut fields = Vec::new();
                        let mut strict = Vec::new();
                        for fld in &cl[1..] {
                            match fld {
                                Sexp::List(x, _) if x.len() == 2 && x[0].sym() == Some("!") => {
                                    fields.push(self.ty(&x[1])?);
                                    strict.push(true);
                                }
                                _ => {
                                    fields.push(self.ty(fld)?);
                                    strict.push(false);
                                }
                            }
                        }
                        d.ctors.push(CtorDecl {
                            name: cn.to_string(),
                            fields,
                            strict,
                        });
                    }
                    _ => return err(c.span(), "expected a constructor or :linear / :newtype"),
                }
            }
            Ok(())
        })();
        self.tyvars.truncate(n);
        r?;
        Ok(d)
    }

    fn extern_decl(&mut self, f: &Sexp) -> R<ExternDecl> {
        let l = f.list().unwrap();
        if l.len() != 4 && l.len() != 5 {
            return err(
                f.span(),
                "expected (extern name (param types) result-type [(c (c types) c-type)])",
            );
        }
        let name = self.sym(&l[1], "an extern name")?.to_string();
        let mut params = Vec::new();
        for p in self.list(&l[2], "parameter types")? {
            params.push(self.ty(p)?);
        }
        let ret = self.ty(&l[3])?;
        let c_sig = match l.get(4) {
            None => None,
            Some(s) => {
                let parts = self.list(s, "a C signature")?;
                let cty = |x: &Sexp| -> R<CTy> {
                    match x {
                        Sexp::Sym(n, _) => CTy::from_name(n).map_or_else(
                            || err(x.span(), "expected a C type (i32, f64, ptr, ...)"),
                            Ok,
                        ),
                        _ => err(x.span(), "expected a C type (i32, f64, ptr, ...)"),
                    }
                };
                if parts.len() != 3 || !matches!(&parts[0], Sexp::Sym(k, _) if k == "c") {
                    return err(s.span(), "expected (c (param C types) result C type)");
                }
                let mut ps = Vec::new();
                for p in self.list(&parts[1], "C parameter types")? {
                    ps.push(cty(p)?);
                }
                Some(CSig {
                    params: ps,
                    ret: cty(&parts[2])?,
                })
            }
        };
        Ok(ExternDecl {
            name,
            params,
            ret,
            c_sig,
        })
    }

    fn def(&mut self, f: &Sexp) -> R<TopBind> {
        let l = f.list().unwrap();
        if l.len() != 4 {
            return err(f.span(), "expected (def name type expr)");
        }
        let name = self.sym(&l[1], "a definition name")?.to_string();
        let ty = self.ty(&l[2])?;
        let expr = self.expr(&l[3])?;
        Ok(TopBind {
            name,
            ty,
            expr,
            span: f.span(),
        })
    }

    /// `(x T)`, `(x m T)` → binder (not yet in scope).
    fn binder(&mut self, s: &Sexp) -> R<Binder> {
        let l = self.list(s, "a binder (x T) or (x m T)")?;
        let (name, m, t) = match l {
            [x, t] => (x, Mult::Many, t),
            [x, m, t] => match mult_of(m) {
                Some(m) => (x, m, t),
                None => return err(m.span(), "expected a multiplicity 0, 1 or ω"),
            },
            _ => return err(s.span(), "expected a binder (x T) or (x m T)"),
        };
        let name = self.sym(name, "a variable name")?.to_string();
        let ty = self.ty(t)?;
        Ok(Binder::new(self.fresh(&name), m, ty))
    }

    fn push(&mut self, b: &Binder) {
        self.vars
            .push((b.id.name.clone(), b.id.clone(), b.ty.clone()));
    }

    fn expr(&mut self, s: &Sexp) -> R<Expr> {
        match s {
            Sexp::Int(n, _) => Ok(Expr::Lit(Lit::Int(*n))),
            Sexp::Float(x, _) => Ok(Expr::Lit(Lit::Double(*x))),
            Sexp::Str(b, _) => Ok(Expr::Lit(Lit::Str(b.clone()))),
            Sexp::Sym(x, _) if x == "unit" => Ok(Expr::Lit(Lit::Unit)),
            Sexp::Sym(x, sp) => {
                if let Some((_, id, _)) = self.vars.iter().rev().find(|(n, _, _)| n == x) {
                    return Ok(Expr::Var(id.clone()));
                }
                if self.globals.contains(x) {
                    return Ok(Expr::Global(x.clone()));
                }
                err(*sp, format!("unbound variable `{x}`"))
            }
            Sexp::List(l, sp) => {
                let Some(head) = l.first() else {
                    return err(*sp, "empty expression");
                };
                let n = l.len();
                match head.sym() {
                    Some("lam") if n == 3 => {
                        let bs = self.list(&l[1], "lambda binders")?;
                        let mark = self.vars.len();
                        let mut binders = Vec::new();
                        for b in bs {
                            let b = self.binder(b)?;
                            self.push(&b);
                            binders.push(b);
                        }
                        let body = self.expr(&l[2]);
                        self.vars.truncate(mark);
                        let mut e = body?;
                        for b in binders.into_iter().rev() {
                            e = Expr::Lam(b, Box::new(e));
                        }
                        Ok(e)
                    }
                    Some("tlam") if n == 3 => {
                        let bs = self.list(&l[1], "type variable binders")?;
                        let mark = self.tyvars.len();
                        let mut vs = Vec::new();
                        for b in bs {
                            let (v, k) = self.tv_binder(b)?;
                            self.tyvars.push(v.clone());
                            vs.push((v, k));
                        }
                        let body = self.expr(&l[2]);
                        self.tyvars.truncate(mark);
                        let mut e = body?;
                        for (v, k) in vs.into_iter().rev() {
                            e = Expr::TyLam(v, k, Box::new(e));
                        }
                        Ok(e)
                    }
                    Some("@") if n >= 3 => {
                        let mut e = self.expr(&l[1])?;
                        for t in &l[2..] {
                            e = Expr::TyApp(Box::new(e), self.ty(t)?);
                        }
                        Ok(e)
                    }
                    Some("let") if n == 3 => {
                        let bs = self.list(&l[1], "let bindings")?;
                        let mark = self.vars.len();
                        let mut binds = Vec::new();
                        let r = (|| {
                            for b in bs {
                                let (bd, rhs) = self.let_binding(b)?;
                                let rhs = self.expr(rhs)?;
                                self.push(&bd);
                                binds.push((bd, rhs));
                            }
                            self.expr(&l[2])
                        })();
                        self.vars.truncate(mark);
                        let mut e = r?;
                        for (b, rhs) in binds.into_iter().rev() {
                            e = Expr::Let(Bind::NonRec(b, Box::new(rhs)), Box::new(e));
                        }
                        Ok(e)
                    }
                    Some("letrec") if n == 3 => {
                        let bs = self.list(&l[1], "letrec bindings")?;
                        let mark = self.vars.len();
                        let r = (|| {
                            let mut pending = Vec::new();
                            for b in bs {
                                let (bd, rhs) = self.let_binding(b)?;
                                self.push(&bd);
                                pending.push((bd, rhs));
                            }
                            let mut binds = Vec::new();
                            for (bd, rhs) in pending {
                                binds.push((bd, self.expr(rhs)?));
                            }
                            let body = self.expr(&l[2])?;
                            Ok(Expr::Let(Bind::Rec(binds), Box::new(body)))
                        })();
                        self.vars.truncate(mark);
                        r
                    }
                    Some("let!") if n == 3 => {
                        let (bd, rhs) = self.let_binding(&l[1])?;
                        let rhs = self.expr(rhs)?;
                        let mark = self.vars.len();
                        self.push(&bd);
                        let body = self.expr(&l[2]);
                        self.vars.truncate(mark);
                        Ok(Expr::LetStrict(bd, Box::new(rhs), Box::new(body?)))
                    }
                    Some("case") if n >= 4 => self.case(l, *sp),
                    Some("con") if n >= 3 => {
                        let c = self.sym(&l[1], "a constructor name")?.to_string();
                        let mut targs = Vec::new();
                        for t in self.list(&l[2], "constructor type arguments")? {
                            targs.push(self.ty(t)?);
                        }
                        let mut args = Vec::new();
                        for a in &l[3..] {
                            args.push(self.expr(a)?);
                        }
                        Ok(Expr::Con(c, targs, args))
                    }
                    Some("prim") if n >= 2 => {
                        let name = self.sym(&l[1], "a primitive name")?;
                        let Some(op) = PrimOp::from_name(name) else {
                            return err(l[1].span(), format!("unknown primitive `{name}`"));
                        };
                        let mut rest = &l[2..];
                        let mut targs = Vec::new();
                        if op.ty_arity() > 0 {
                            let Some(ts) = rest.first() else {
                                return err(*sp, format!("`{name}` needs type arguments"));
                            };
                            for t in self.list(ts, "type arguments")? {
                                targs.push(self.ty(t)?);
                            }
                            rest = &rest[1..];
                        }
                        let mut args = Vec::new();
                        for a in rest {
                            args.push(self.expr(a)?);
                        }
                        Ok(Expr::Prim(op, targs, args))
                    }
                    Some("extern") if n >= 2 => {
                        let name = self.sym(&l[1], "an extern name")?.to_string();
                        let mut args = Vec::new();
                        for a in &l[2..] {
                            args.push(self.expr(a)?);
                        }
                        Ok(Expr::Extern(name, args))
                    }
                    Some("cast") if n == 3 => {
                        let e = self.expr(&l[1])?;
                        Ok(Expr::Cast(Box::new(e), self.ty(&l[2])?))
                    }
                    Some("panic") if n == 3 => {
                        let Sexp::Str(m, _) = &l[1] else {
                            return err(l[1].span(), "expected a message string");
                        };
                        let msg = String::from_utf8_lossy(m).into_owned();
                        Ok(Expr::Panic(msg, self.ty(&l[2])?))
                    }
                    Some(
                        kw @ ("lam" | "tlam" | "@" | "let" | "letrec" | "let!" | "case" | "con"
                        | "prim" | "extern" | "cast" | "panic"),
                    ) => err(*sp, format!("malformed `{kw}` expression")),
                    _ => {
                        let f = self.expr(head)?;
                        let mut args = Vec::new();
                        for a in &l[1..] {
                            args.push(self.expr(a)?);
                        }
                        if args.is_empty() {
                            return err(*sp, "application without arguments");
                        }
                        Ok(Expr::app(f, args))
                    }
                }
            }
        }
    }

    /// `(x T rhs)` / `(x m T rhs)` → binder and right-hand side.
    fn let_binding<'a>(&mut self, s: &'a Sexp) -> R<(Binder, &'a Sexp)> {
        let l = self.list(s, "a binding (x T rhs)")?;
        let (name, m, t, rhs) = match l {
            [x, t, rhs] => (x, Mult::Many, t, rhs),
            [x, m, t, rhs] => match mult_of(m) {
                Some(m) => (x, m, t, rhs),
                None => return err(m.span(), "expected a multiplicity 0, 1 or ω"),
            },
            _ => return err(s.span(), "expected a binding (x T rhs) or (x m T rhs)"),
        };
        let name = self.sym(name, "a variable name")?.to_string();
        let ty = self.ty(t)?;
        Ok((Binder::new(self.fresh(&name), m, ty), rhs))
    }

    fn case(&mut self, l: &[Sexp], sp: Span) -> R<Expr> {
        let scrut = self.expr(&l[1])?;
        let b = self.binder(&l[2])?;
        let res = self.ty(&l[3])?;
        let mark = self.vars.len();
        self.push(&b);
        let r = (|| {
            let mut alts = Vec::new();
            for a in &l[4..] {
                let al = self.list(a, "a case alternative (pattern expr)")?;
                if al.len() != 2 {
                    return err(a.span(), "expected (pattern expr)");
                }
                let inner = self.vars.len();
                let con = match &al[0] {
                    Sexp::Sym(s, _) if s == "_" => AltCon::Default,
                    Sexp::Int(n, _) => AltCon::Int(*n),
                    Sexp::List(pl, psp) => {
                        let Some(cn) = pl.first().and_then(|x| x.sym()) else {
                            return err(*psp, "expected (Ctor fields...)");
                        };
                        let ftys = self.field_types(cn, &b.ty, *psp)?;
                        if ftys.len() != pl.len() - 1 {
                            return err(
                                *psp,
                                format!(
                                    "constructor `{cn}` has {} fields, pattern binds {}",
                                    ftys.len(),
                                    pl.len() - 1
                                ),
                            );
                        }
                        let mut bs = Vec::new();
                        for (fx, fty) in pl[1..].iter().zip(ftys) {
                            let (name, m) = match fx {
                                Sexp::Sym(x, _) => (x.clone(), Mult::Many),
                                Sexp::List(x, _) if x.len() == 2 => {
                                    let Some(m) = mult_of(&x[1]) else {
                                        return err(x[1].span(), "expected a multiplicity");
                                    };
                                    (self.sym(&x[0], "a field variable")?.to_string(), m)
                                }
                                _ => return err(fx.span(), "expected a field variable"),
                            };
                            let bd = Binder::new(self.fresh(&name), m, fty);
                            self.push(&bd);
                            bs.push(bd);
                        }
                        AltCon::Con(cn.to_string(), bs)
                    }
                    p => return err(p.span(), "expected a pattern"),
                };
                let rhs = self.expr(&al[1]);
                self.vars.truncate(inner);
                alts.push(Alt { con, rhs: rhs? });
            }
            Ok(alts)
        })();
        self.vars.truncate(mark);
        let alts = r?;
        if alts.is_empty() {
            return err(sp, "case without alternatives");
        }
        Ok(Expr::Case(Box::new(scrut), b, res, alts))
    }

    /// Field types of constructor `c` at scrutinee type `t`.
    fn field_types(&self, c: &str, t: &Type, sp: Span) -> R<Vec<Type>> {
        let Some(d) = self
            .datas
            .iter()
            .find(|d| d.ctors.iter().any(|k| k.name == c))
        else {
            return err(sp, format!("unknown constructor `{c}`"));
        };
        let Type::Con(tn, args) = t else {
            return err(
                sp,
                format!(
                    "pattern `{c}` needs the case binder to have type `{}`",
                    d.name
                ),
            );
        };
        if tn != &d.name || args.len() != d.params.len() {
            return err(
                sp,
                format!("constructor `{c}` does not belong to type `{t}`"),
            );
        }
        let k = d.ctors.iter().find(|k| k.name == c).unwrap();
        let s: Vec<(TyVar, Type)> = d
            .params
            .iter()
            .map(|p| p.0.clone())
            .zip(args.iter().cloned())
            .collect();
        Ok(k.fields.iter().map(|f| f.subst_all(&s)).collect())
    }
}
