//! Core Lint (06-ir.md §1): types, kinds, multiplicities, U-Bind and scopes.
//!
//! Multiplicities follow Linear Haskell's λ^q: every expression has a
//! *usage* (how often it uses each variable); `f a` with `f : A →π B` uses
//! `usage(f) + π·usage(a)`, `let x :π = e₁ in e₂` uses `π·usage(e₁) +
//! usage(e₂)`, and the branches of a `case` must agree. A binder with
//! multiplicity 1 must be used exactly once, one with multiplicity 0 never.
//!
//! Additional aCHC / CirkaLang rules:
//! - **U-Bind**: a binder of multiplicity ω must have a type of kind `Type`
//!   (03-type-system.md §2.2).
//! - **Strict-Linear**: a lazy `let` whose right-hand side (not a value)
//!   uses a linear variable, or whose type is `LType`, is rejected (§6).
//! - `cast` only between boxed types of compatible kinds; function types
//!   must agree on unboxed argument/result positions.

use crate::expr::*;
use crate::ty::*;
use cirka_diagnostics::Diagnostic;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Lints a whole program; returns all errors (at most one per definition).
pub fn lint_program(p: &Program) -> Vec<Diagnostic> {
    let mut l = Linter::new(p);
    l.program();
    l.errs
}

/// Lints one top-level binding of `p` (by index) against the rest of the
/// program; used by the optimizer to check each binding after a pass.
pub fn lint_bind(p: &Program, i: usize) -> Vec<Diagnostic> {
    let mut l = Linter::new(p);
    l.top(&p.binds[i]);
    l.errs
}

/// Renders lint errors as plain text (tests and ICE reports).
pub fn render_errors(errs: &[Diagnostic]) -> String {
    let sm = cirka_diagnostics::SourceMap::new();
    errs.iter()
        .map(|d| cirka_diagnostics::render(d, &sm, cirka_diagnostics::Style::Plain))
        .collect()
}

/// How often an expression uses each variable.
#[derive(Clone, Debug, Default, PartialEq)]
struct Usage {
    map: BTreeMap<u32, Mult>,
    /// The expression diverges (`panic`): it may use anything.
    bottom: bool,
}

impl Usage {
    fn one(v: &Id) -> Usage {
        let mut u = Usage::default();
        u.map.insert(v.id, Mult::One);
        u
    }

    fn bottom() -> Usage {
        Usage {
            map: BTreeMap::new(),
            bottom: true,
        }
    }

    fn add(mut self, o: Usage) -> Usage {
        for (k, m) in o.map {
            let e = self.map.entry(k).or_insert(Mult::Zero);
            *e = e.plus(m);
        }
        self.bottom |= o.bottom;
        self
    }

    fn scale(mut self, m: Mult) -> Usage {
        if m == Mult::Zero {
            return Usage {
                map: BTreeMap::new(),
                bottom: self.bottom,
            };
        }
        for v in self.map.values_mut() {
            *v = v.times(m);
        }
        self
    }

    fn remove(&mut self, v: &Id) -> Mult {
        self.map.remove(&v.id).unwrap_or(Mult::Zero)
    }

    /// Join of case branches: equal usages are kept; a mismatch becomes
    /// `None` for that variable (reported when its binder is checked).
    fn join(branches: Vec<Usage>) -> (Usage, Vec<u32>) {
        let live: Vec<&Usage> = branches.iter().filter(|u| !u.bottom).collect();
        if live.is_empty() {
            return (Usage::bottom(), vec![]);
        }
        let mut keys: HashSet<u32> = HashSet::new();
        for u in &live {
            keys.extend(u.map.keys().copied());
        }
        let mut out = Usage::default();
        let mut mismatched = Vec::new();
        for k in keys {
            let ms: Vec<Mult> = live
                .iter()
                .map(|u| u.map.get(&k).copied().unwrap_or(Mult::Zero))
                .collect();
            if ms.iter().all(|m| *m == ms[0]) {
                if ms[0] != Mult::Zero {
                    out.map.insert(k, ms[0]);
                }
            } else {
                mismatched.push(k);
                out.map.insert(k, Mult::Many);
            }
        }
        (out, mismatched)
    }
}

type R<T> = Result<T, String>;

struct Linter<'p> {
    p: &'p Program,
    errs: Vec<Diagnostic>,
    globals: HashMap<&'p str, &'p Type>,
    vars: HashMap<u32, (Id, Mult, Type)>,
    tyvars: Vec<(TyVar, Kind)>,
    /// Variables whose usage differed between case branches.
    inconsistent: HashSet<u32>,
}

impl<'p> Linter<'p> {
    fn new(p: &'p Program) -> Linter<'p> {
        Linter {
            p,
            errs: Vec::new(),
            globals: p.binds.iter().map(|b| (b.name.as_str(), &b.ty)).collect(),
            vars: HashMap::new(),
            tyvars: Vec::new(),
            inconsistent: HashSet::new(),
        }
    }

    fn error(&mut self, ctx: &str, msg: String) {
        self.errs
            .push(Diagnostic::error(format!("Core Lint: in {ctx}: {msg}")).with_code("C0100"));
    }

    fn program(&mut self) {
        let p = self.p;
        let mut seen = HashSet::new();
        let mut ctors = HashSet::new();
        for d in &p.datas {
            if !seen.insert(d.name.as_str()) {
                self.error(&format!("data `{}`", d.name), "defined twice".into());
            }
            for c in &d.ctors {
                if !ctors.insert(c.name.as_str()) {
                    self.error(
                        &format!("data `{}`", d.name),
                        format!("constructor `{}` defined twice", c.name),
                    );
                }
            }
            if let Err(e) = self.data(d) {
                self.error(&format!("data `{}`", d.name), e);
            }
        }
        for e in &p.externs {
            let r = (|| {
                for t in e.params.iter().chain(std::iter::once(&e.ret)) {
                    let k = self.kind(t)?;
                    if matches!(k, Kind::Arrow(..)) {
                        return Err(format!("`{t}` is not a type of values"));
                    }
                }
                Ok(())
            })();
            if let Err(m) = r {
                self.error(&format!("extern `{}`", e.name), m);
            }
        }
        let mut names = HashSet::new();
        for b in &p.binds {
            if !names.insert(b.name.as_str()) {
                self.error(&format!("`{}`", b.name), "defined twice".into());
                continue;
            }
            self.top(b);
        }
        match p.bind("main") {
            Some(m) if m.ty.alpha_eq(&Type::int()) => {}
            Some(m) => {
                let t = m.ty.to_string();
                self.error("`main`", format!("must have type Int#, not {t}"));
            }
            None => {}
        }
    }

    fn top(&mut self, b: &TopBind) {
        self.vars.clear();
        self.tyvars.clear();
        self.inconsistent.clear();
        let r = (|| {
            let k = self.type_kind(&b.ty)?;
            if k == Kind::LType {
                return Err(format!(
                    "top-level bindings are unrestricted (ω), but `{}` has a linear type",
                    b.ty
                ));
            }
            let (t, _) = self.expr(&b.expr)?;
            if !t.alpha_eq(&b.ty) {
                return Err(format!("declared type {} but the body has type {t}", b.ty));
            }
            Ok(())
        })();
        if let Err(m) = r {
            self.error(&format!("`{}`", b.name), m);
        }
    }

    fn data(&mut self, d: &DataDecl) -> R<()> {
        self.tyvars.clear();
        for (v, k) in &d.params {
            if *k == Kind::Unboxed {
                return Err(format!("type parameter `{}` cannot have kind #", v.name));
            }
            self.tyvars.push((v.clone(), k.clone()));
        }
        for c in &d.ctors {
            if c.strict.len() != c.fields.len() {
                return Err(format!(
                    "constructor `{}`: strictness flags do not match fields",
                    c.name
                ));
            }
            for f in &c.fields {
                let k = self.kind(f)?;
                if matches!(k, Kind::Arrow(..)) {
                    return Err(format!(
                        "field type `{f}` of `{}` is not a type of values",
                        c.name
                    ));
                }
            }
        }
        if d.newtype && (d.ctors.len() != 1 || d.ctors[0].fields.len() != 1) {
            return Err("a newtype must have exactly one constructor with one field".into());
        }
        self.tyvars.clear();
        Ok(())
    }

    // ---- kinds ----

    fn tyvar_kind(&self, v: &TyVar) -> R<Kind> {
        self.tyvars
            .iter()
            .rev()
            .find(|(x, _)| x == v)
            .map(|(_, k)| k.clone())
            .ok_or_else(|| format!("type variable `{}` is not in scope", v.name))
    }

    /// Kind of a type of values (Type, LType or #).
    fn type_kind(&self, t: &Type) -> R<Kind> {
        let k = self.kind(t)?;
        if matches!(k, Kind::Arrow(..)) {
            return Err(format!("`{t}` is not a type of values (kind {k})"));
        }
        Ok(k)
    }

    fn kind(&self, t: &Type) -> R<Kind> {
        match t {
            Type::Var(v) => self.tyvar_kind(v),
            Type::Any => Ok(Kind::Type),
            Type::Fun(_, a, b) => {
                self.type_kind(a)?;
                self.type_kind(b)?;
                Ok(Kind::Type)
            }
            Type::Forall(v, k, b) => {
                let mut inner = Linter {
                    p: self.p,
                    errs: vec![],
                    globals: HashMap::new(),
                    vars: HashMap::new(),
                    tyvars: self.tyvars.clone(),
                    inconsistent: HashSet::new(),
                };
                inner.tyvars.push((v.clone(), k.clone()));
                inner.type_kind(b)
            }
            Type::App(h, args) => {
                let hk = self.kind(h)?;
                self.apply_kind(hk, args, t)
            }
            Type::Con(c, args) => {
                let builtin = match c.as_str() {
                    INT | DOUBLE | UNIT => Some((0, Kind::Unboxed)),
                    BYTES => Some((0, Kind::Type)),
                    ARRAY => Some((1, Kind::Type)),
                    _ => None,
                };
                if let Some((n, k)) = builtin {
                    if args.len() != n {
                        return Err(format!("`{c}` takes {n} type arguments"));
                    }
                    if c == ARRAY {
                        let ek = self.type_kind(&args[0])?;
                        if ek == Kind::Unboxed {
                            return Err("array elements must be boxed".into());
                        }
                        return Ok(ek);
                    }
                    return Ok(k);
                }
                let Some(d) = self.p.data(c) else {
                    return Err(format!("unknown type constructor `{c}`"));
                };
                if args.len() > d.params.len() {
                    return Err(format!(
                        "`{c}` takes {} type arguments, got {}",
                        d.params.len(),
                        args.len()
                    ));
                }
                for (a, (_, pk)) in args.iter().zip(&d.params) {
                    let ak = self.kind(a)?;
                    if !ak.sub(pk) {
                        return Err(format!(
                            "type argument `{a}` of `{c}` has kind {ak}, expected {pk}"
                        ));
                    }
                }
                if args.len() < d.params.len() {
                    // Partial application: an arrow kind.
                    let mut k = Kind::Type;
                    for (_, pk) in d.params[args.len()..].iter().rev() {
                        k = Kind::Arrow(Box::new(pk.clone()), Box::new(k));
                    }
                    return Ok(k);
                }
                Ok(self.data_kind(d, args, &mut Vec::new()))
            }
        }
    }

    fn apply_kind(&self, mut k: Kind, args: &[Type], t: &Type) -> R<Kind> {
        for a in args {
            match k {
                Kind::Arrow(pk, rk) => {
                    let ak = self.kind(a)?;
                    if !ak.sub(&pk) {
                        return Err(format!(
                            "in `{t}`: argument `{a}` has kind {ak}, expected {pk}"
                        ));
                    }
                    k = *rk;
                }
                _ => {
                    return Err(format!(
                        "`{t}` applies a type that is not a type constructor"
                    ));
                }
            }
        }
        Ok(k)
    }

    /// Kind of a fully applied data type: LType when declared linear or when
    /// a field is linear (03-type-system.md §1.1).
    fn data_kind(&self, d: &DataDecl, args: &[Type], visiting: &mut Vec<String>) -> Kind {
        if d.linear {
            return Kind::LType;
        }
        if visiting.contains(&d.name) {
            return Kind::Type;
        }
        visiting.push(d.name.clone());
        let s: Vec<(TyVar, Type)> = d
            .params
            .iter()
            .map(|p| p.0.clone())
            .zip(args.iter().cloned())
            .collect();
        let mut k = Kind::Type;
        for c in &d.ctors {
            for f in &c.fields {
                let ft = f.subst_all(&s);
                if self.field_linear(&ft, visiting) {
                    k = Kind::LType;
                }
            }
        }
        visiting.pop();
        k
    }

    fn field_linear(&self, t: &Type, visiting: &mut Vec<String>) -> bool {
        match t {
            Type::Con(c, args) => match self.p.data(c) {
                Some(d) if args.len() == d.params.len() => {
                    self.data_kind(d, args, visiting) == Kind::LType
                }
                _ => c == ARRAY && args.first().is_some_and(|a| self.field_linear(a, visiting)),
            },
            _ => matches!(self.kind(t), Ok(Kind::LType)),
        }
    }

    // ---- expressions ----

    fn bind_var(&mut self, b: &Binder) -> R<()> {
        let k = self.type_kind(&b.ty)?;
        if b.mult == Mult::Many && k == Kind::LType {
            return Err(format!(
                "U-Bind: `{}` has linear type {} but is bound with multiplicity ω",
                b.id.name, b.ty
            ));
        }
        if self.vars.contains_key(&b.id.id) {
            return Err(format!(
                "variable `{}` (#{}) is bound twice",
                b.id.name, b.id.id
            ));
        }
        self.vars
            .insert(b.id.id, (b.id.clone(), b.mult, b.ty.clone()));
        Ok(())
    }

    /// Checks the usage of a binder that goes out of scope.
    fn check_usage(&mut self, b: &Binder, used: Mult) -> R<()> {
        self.vars.remove(&b.id.id);
        if self.inconsistent.remove(&b.id.id) && b.mult != Mult::Many {
            return Err(format!(
                "linear variable `{}` is used differently in the branches of a case",
                b.id.name
            ));
        }
        match (b.mult, used) {
            (Mult::One, Mult::One) | (Mult::Zero, Mult::Zero) | (Mult::Many, _) => Ok(()),
            (Mult::One, Mult::Zero) => {
                Err(format!("linear variable `{}` is never used", b.id.name))
            }
            (Mult::One, _) => Err(format!(
                "linear variable `{}` is used more than once",
                b.id.name
            )),
            (Mult::Zero, _) => Err(format!(
                "erased variable `{}` (multiplicity 0) is used",
                b.id.name
            )),
        }
    }

    fn expect(&self, what: &str, got: &Type, want: &Type) -> R<()> {
        if got.alpha_eq(want) {
            Ok(())
        } else {
            Err(format!("{what} has type {got}, expected {want}"))
        }
    }

    fn expr(&mut self, e: &Expr) -> R<(Type, Usage)> {
        match e {
            Expr::Var(v) => match self.vars.get(&v.id) {
                Some((id, _, t)) if id.name == v.name => Ok((t.clone(), Usage::one(v))),
                _ => Err(format!("variable `{}` (#{}) is not in scope", v.name, v.id)),
            },
            Expr::Global(g) => match self.globals.get(g.as_str()) {
                Some(t) => Ok(((*t).clone(), Usage::default())),
                None => Err(format!("unknown global `{g}`")),
            },
            Expr::Lit(l) => Ok((l.ty(), Usage::default())),
            Expr::Con(c, targs, args) => {
                let Some((d, i)) = self.p.ctor(c) else {
                    return Err(format!("unknown constructor `{c}`"));
                };
                if targs.len() != d.params.len() {
                    return Err(format!(
                        "constructor `{c}` needs {} type arguments, got {}",
                        d.params.len(),
                        targs.len()
                    ));
                }
                for (a, (_, pk)) in targs.iter().zip(&d.params) {
                    let ak = self.kind(a)?;
                    if !ak.sub(pk) {
                        return Err(format!(
                            "type argument `{a}` of `{c}` has kind {ak}, expected {pk}"
                        ));
                    }
                }
                let k = &d.ctors[i];
                if args.len() != k.fields.len() {
                    return Err(format!(
                        "constructor `{c}` has {} fields, got {} arguments",
                        k.fields.len(),
                        args.len()
                    ));
                }
                let s: Vec<(TyVar, Type)> = d
                    .params
                    .iter()
                    .map(|p| p.0.clone())
                    .zip(targs.iter().cloned())
                    .collect();
                let mut u = Usage::default();
                for (j, (a, f)) in args.iter().zip(&k.fields).enumerate() {
                    let (t, ua) = self.expr(a)?;
                    self.expect(&format!("field {j} of `{c}`"), &t, &f.subst_all(&s))?;
                    u = u.add(ua);
                }
                Ok((Type::Con(d.name.clone(), targs.clone()), u))
            }
            Expr::App(f, a) => {
                let (tf, uf) = self.expr(f)?;
                let Type::Fun(m, pa, rb) = tf else {
                    return Err(format!("applying a value of non-function type {tf}"));
                };
                let (ta, ua) = self.expr(a)?;
                self.expect("argument", &ta, &pa)?;
                Ok((*rb, uf.add(ua.scale(m))))
            }
            Expr::TyApp(f, t) => {
                let (tf, uf) = self.expr(f)?;
                let Type::Forall(v, k, b) = tf else {
                    return Err(format!("type application of a value of type {tf}"));
                };
                let tk = self.kind(t)?;
                if !tk.sub(&k) {
                    return Err(format!("type argument `{t}` has kind {tk}, expected {k}"));
                }
                Ok((b.subst(&v, t), uf))
            }
            Expr::Lam(b, body) => {
                self.bind_var(b)?;
                let (tb, mut ub) = self.expr(body)?;
                let used = ub.remove(&b.id);
                self.check_usage(b, used)?;
                Ok((Type::fun(b.mult, b.ty.clone(), tb), ub))
            }
            Expr::TyLam(v, k, body) => {
                if *k == Kind::Unboxed {
                    return Err(format!("type variable `{}` cannot have kind #", v.name));
                }
                self.tyvars.push((v.clone(), k.clone()));
                let r = self.expr(body);
                self.tyvars.pop();
                let (t, u) = r?;
                Ok((Type::Forall(v.clone(), k.clone(), Box::new(t)), u))
            }
            Expr::Let(Bind::NonRec(b, rhs), body) => {
                let (tr, ur) = self.expr(rhs)?;
                self.expect(&format!("right-hand side of `{}`", b.id.name), &tr, &b.ty)?;
                if !rhs.is_value() {
                    let k = self.type_kind(&b.ty)?;
                    if k == Kind::LType || ur.map.iter().any(|(v, _)| self.is_linear(*v)) {
                        return Err(format!(
                            "Strict-Linear: lazy `let {}` would capture a linear value (use `let!`)",
                            b.id.name
                        ));
                    }
                    if k == Kind::Unboxed {
                        return Err(format!(
                            "lazy `let {}` of unboxed type {} (use `let!`)",
                            b.id.name, b.ty
                        ));
                    }
                }
                self.bind_var(b)?;
                let (tb, mut ub) = self.expr(body)?;
                let used = ub.remove(&b.id);
                self.check_usage(b, used)?;
                Ok((tb, ub.add(ur.scale(b.mult))))
            }
            Expr::Let(Bind::Rec(bs), body) => {
                for (b, _) in bs {
                    if b.mult != Mult::Many {
                        return Err(format!(
                            "recursive binding `{}` must have multiplicity ω",
                            b.id.name
                        ));
                    }
                    self.bind_var(b)?;
                }
                let mut u = Usage::default();
                for (b, rhs) in bs {
                    let (tr, ur) = self.expr(rhs)?;
                    self.expect(&format!("right-hand side of `{}`", b.id.name), &tr, &b.ty)?;
                    if !rhs.is_value() && self.type_kind(&b.ty)? == Kind::Unboxed {
                        return Err(format!("recursive binding `{}` of unboxed type", b.id.name));
                    }
                    u = u.add(ur.scale(Mult::Many));
                }
                let (tb, ub) = self.expr(body)?;
                let mut u = u.add(ub);
                for (b, _) in bs {
                    u.remove(&b.id);
                    self.vars.remove(&b.id.id);
                }
                // Linear variables captured by recursive bindings are used ω.
                for (v, m) in &u.map {
                    if *m == Mult::Many && self.is_linear(*v) {
                        let name = self.vars[v].0.name.clone();
                        return Err(format!(
                            "linear variable `{name}` is captured by a recursive binding"
                        ));
                    }
                }
                Ok((tb, u))
            }
            Expr::LetStrict(b, rhs, body) => {
                let (tr, ur) = self.expr(rhs)?;
                self.expect(&format!("right-hand side of `{}`", b.id.name), &tr, &b.ty)?;
                self.bind_var(b)?;
                let (tb, mut ub) = self.expr(body)?;
                let used = ub.remove(&b.id);
                self.check_usage(b, used)?;
                // Like `case`: evaluated once; the result may only be
                // duplicated when it cannot carry linear resources.
                let m = self.consumption(b);
                Ok((tb, ub.add(ur.scale(m))))
            }
            Expr::Case(scrut, b, res, alts) => self.case(scrut, b, res, alts),
            Expr::Cast(e, t) => {
                let (te, u) = self.expr(e)?;
                self.cast_ok(&te, t)?;
                Ok((t.clone(), u))
            }
            Expr::Prim(op, targs, args) => {
                if targs.len() != op.ty_arity() {
                    return Err(format!(
                        "`{}` takes {} type arguments",
                        op.name(),
                        op.ty_arity()
                    ));
                }
                for t in targs {
                    if self.type_kind(t)? == Kind::Unboxed {
                        return Err(format!("`{}` needs a boxed type argument", op.name()));
                    }
                }
                let (ps, r) = op.signature(targs);
                if ps.len() != args.len() {
                    return Err(format!(
                        "`{}` takes {} arguments, got {}",
                        op.name(),
                        ps.len(),
                        args.len()
                    ));
                }
                let mut u = Usage::default();
                for (i, (a, p)) in args.iter().zip(&ps).enumerate() {
                    let (t, ua) = self.expr(a)?;
                    self.expect(&format!("argument {i} of `{}`", op.name()), &t, p)?;
                    u = u.add(ua);
                }
                Ok((r, u))
            }
            Expr::Extern(name, args) => {
                let Some(d) = self.p.extern_decl(name) else {
                    return Err(format!("undeclared extern `{name}`"));
                };
                if d.params.len() != args.len() {
                    return Err(format!(
                        "extern `{name}` takes {} arguments, got {}",
                        d.params.len(),
                        args.len()
                    ));
                }
                let mut u = Usage::default();
                for (i, (a, p)) in args.iter().zip(&d.params).enumerate() {
                    let (t, ua) = self.expr(a)?;
                    self.expect(&format!("argument {i} of `{name}`"), &t, p)?;
                    u = u.add(ua);
                }
                Ok((d.ret.clone(), u))
            }
            Expr::Panic(_, t) => {
                self.type_kind(t)?;
                Ok((t.clone(), Usage::bottom()))
            }
        }
    }

    /// How often the evaluated expression bound to `b` (by `let!` or
    /// `case`) is used: `b`'s multiplicity, except that values that cannot
    /// hold linear resources (no closures inside) count as used once.
    fn consumption(&self, b: &Binder) -> Mult {
        if b.mult == Mult::Many && !self.resource_free(&b.ty, &mut Vec::new()) {
            Mult::Many
        } else {
            Mult::One
        }
    }

    /// True if values of type `t` never contain closures (or values of
    /// unknown type), so they cannot capture linear variables.
    fn resource_free(&self, t: &Type, visiting: &mut Vec<String>) -> bool {
        match t {
            Type::Con(c, args) => {
                if t.is_unboxed() || c == BYTES {
                    return true;
                }
                if c == ARRAY {
                    return args.iter().all(|a| self.resource_free(a, visiting));
                }
                let Some(d) = self.p.data(c) else {
                    return false;
                };
                if args.len() != d.params.len() {
                    return false;
                }
                if visiting.contains(c) {
                    return true;
                }
                visiting.push(c.clone());
                let s: Vec<(TyVar, Type)> = d
                    .params
                    .iter()
                    .map(|p| p.0.clone())
                    .zip(args.iter().cloned())
                    .collect();
                let ok = d.ctors.iter().all(|k| {
                    k.fields
                        .iter()
                        .all(|f| self.resource_free(&f.subst_all(&s), visiting))
                });
                visiting.pop();
                ok
            }
            _ => false,
        }
    }

    fn is_linear(&self, v: u32) -> bool {
        self.vars.get(&v).is_some_and(|(_, m, _)| *m == Mult::One)
    }

    fn case(&mut self, scrut: &Expr, b: &Binder, res: &Type, alts: &[Alt]) -> R<(Type, Usage)> {
        let (ts, us) = self.expr(scrut)?;
        self.expect("scrutinee", &ts, &b.ty)?;
        self.type_kind(res)?;
        self.bind_var(b)?;
        let data = match &b.ty {
            Type::Con(c, args) => self.p.data(c).map(|d| (d, args.clone())),
            _ => None,
        };
        let mut seen: Vec<&AltCon> = Vec::new();
        let mut usages = Vec::new();
        for a in alts {
            if seen.iter().any(|x| match (*x, &a.con) {
                (AltCon::Con(c, _), AltCon::Con(d, _)) => c == d,
                (x, y) => x == y,
            }) {
                return Err("duplicate case alternative".into());
            }
            seen.push(&a.con);
            let mut fields: &[Binder] = &[];
            match &a.con {
                AltCon::Default => {}
                AltCon::Int(_) => {
                    if !b.ty.alpha_eq(&Type::int()) {
                        return Err(format!("integer pattern on a scrutinee of type {}", b.ty));
                    }
                }
                AltCon::Con(c, bs) => {
                    let Some((d, args)) = &data else {
                        return Err(format!(
                            "constructor pattern `{c}` on a scrutinee of type {}",
                            b.ty
                        ));
                    };
                    let Some(k) = d.ctors.iter().find(|k| &k.name == c) else {
                        return Err(format!(
                            "constructor `{c}` does not belong to type {}",
                            b.ty
                        ));
                    };
                    if args.len() != d.params.len() {
                        return Err(format!("scrutinee type {} is not fully applied", b.ty));
                    }
                    if bs.len() != k.fields.len() {
                        return Err(format!(
                            "pattern `{c}` binds {} fields, constructor has {}",
                            bs.len(),
                            k.fields.len()
                        ));
                    }
                    let s: Vec<(TyVar, Type)> = d
                        .params
                        .iter()
                        .map(|p| p.0.clone())
                        .zip(args.iter().cloned())
                        .collect();
                    for (fb, ft) in bs.iter().zip(&k.fields) {
                        self.expect(
                            &format!("pattern variable `{}`", fb.id.name),
                            &fb.ty,
                            &ft.subst_all(&s),
                        )?;
                        self.bind_var(fb)?;
                    }
                    fields = bs;
                }
            }
            let (t, mut u) = self.expr(&a.rhs)?;
            self.expect("case alternative", &t, res)?;
            for fb in fields {
                let used = u.remove(&fb.id);
                if !u.bottom {
                    self.check_usage(fb, used)?;
                } else {
                    self.vars.remove(&fb.id.id);
                }
            }
            // The case binder: when the fields carry the value, a linear
            // binder must not be used as well.
            let ub = u.remove(&b.id);
            if !u.bottom && b.mult == Mult::One {
                let ok = if fields.iter().any(|f| f.mult == Mult::One) {
                    ub == Mult::Zero
                } else if matches!(a.con, AltCon::Default) {
                    ub == Mult::One
                } else {
                    ub != Mult::Many
                };
                if !ok {
                    return Err(format!(
                        "linear case binder `{}` is used incorrectly",
                        b.id.name
                    ));
                }
            } else if !u.bottom && b.mult == Mult::Zero && ub != Mult::Zero {
                return Err(format!("erased case binder `{}` is used", b.id.name));
            }
            usages.push(u);
        }
        self.vars.remove(&b.id.id);
        let (u, mismatched) = Usage::join(usages);
        for v in mismatched {
            if self.is_linear(v) {
                self.inconsistent.insert(v);
            }
        }
        let m = self.consumption(b);
        Ok((res.clone(), us.scale(m).add(u)))
    }

    fn cast_ok(&self, from: &Type, to: &Type) -> R<()> {
        if from.alpha_eq(to) {
            return Ok(());
        }
        let kf = self.type_kind(from)?;
        let kt = self.type_kind(to)?;
        if kf == Kind::Unboxed || kt == Kind::Unboxed {
            return Err(format!(
                "cast between {from} and {to}: unboxed types cannot be cast"
            ));
        }
        if kf != kt {
            return Err(format!(
                "cast from {from} ({kf}) to {to} ({kt}) changes linearity"
            ));
        }
        if let (Type::Fun(_, a1, r1), Type::Fun(_, a2, r2)) = (strip_forall(from), strip_forall(to))
        {
            if a1.is_unboxed() != a2.is_unboxed() || a1.rep() != a2.rep() || r1.rep() != r2.rep() {
                return Err(format!(
                    "cast from {from} to {to} changes the calling convention"
                ));
            }
            if matches!(**r1, Type::Fun(..)) && matches!(**r2, Type::Fun(..)) {
                return self.cast_ok(r1, r2);
            }
        }
        Ok(())
    }
}

fn strip_forall(t: &Type) -> &Type {
    match t {
        Type::Forall(_, _, b) => strip_forall(b),
        t => t,
    }
}
