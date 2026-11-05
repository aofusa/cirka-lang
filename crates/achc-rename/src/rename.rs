//! Renaming of one module at a time (modules come in dependency order).

use crate::fixity::{self, Build, Conflict, Fixity};
use crate::scope::{Ent, Exports, Kind, Ns, Scope};
use crate::{PRIM, err};
use achc_syntax::ast::*;
use cirka_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// The source name of a (possibly unique-renamed) local: `x#3` is `x`.
pub fn display_name(s: &str) -> &str {
    match s.rfind('#') {
        Some(i) if i > 0 && s[i + 1..].chars().all(|c| c.is_ascii_digit()) => &s[..i],
        _ => s,
    }
}

fn is_tuple_name(n: &str) -> bool {
    n.starts_with("(,") && n.ends_with(')') && n[1..n.len() - 1].chars().all(|c| c == ',')
}

/// Built-in syntax in the value namespace.
fn builtin_value(n: &str) -> bool {
    matches!(n, "[]" | ":" | "()") || is_tuple_name(n)
}

fn builtin_type(n: &str) -> bool {
    matches!(n, "[]" | "()" | "->") || is_tuple_name(n)
}

/// The base library module that defines the Prelude's entities.
pub const BASE_MODULE: &str = "Cirka.Base";

pub struct Renamer {
    pub errors: Vec<Diagnostic>,
    exports: HashMap<String, Exports>,
    children: HashMap<(String, String), Vec<Ent>>,
    fixities: HashMap<(String, String), Fixity>,
    module: String,
    scope: Scope,
    locals: Vec<HashMap<String, String>>,
    local_fix: HashMap<String, Fixity>,
    counter: u32,
    /// Patterns bind globals (top-level pattern bindings).
    top_pat: bool,
}

struct ExprBuilder<'a>(&'a Renamer);

impl Build<Expr> for ExprBuilder<'_> {
    fn fixity(&self, op: &QName) -> Fixity {
        self.0.fixity_of(op)
    }
    fn binary(&mut self, op: QName, l: Expr, r: Expr) -> Expr {
        let span = l.span.to(r.span);
        let f = Expr {
            span: op.span,
            kind: if op.is_con() {
                ExprKind::Con(op)
            } else {
                ExprKind::Var(op)
            },
        };
        let lspan = f.span.to(l.span);
        Expr {
            kind: ExprKind::App(
                Box::new(Expr {
                    kind: ExprKind::App(Box::new(f), Box::new(l)),
                    span: lspan,
                }),
                Box::new(r),
            ),
            span,
        }
    }
    fn negate(&mut self, span: Span, e: Expr) -> Result<Expr, ()> {
        let neg = Expr {
            kind: ExprKind::Var(QName {
                qual: Some(self.0.base_module().into()),
                name: "negate".into(),
                span,
            }),
            span,
        };
        let sp = span.to(e.span);
        Ok(Expr {
            kind: ExprKind::App(Box::new(neg), Box::new(e)),
            span: sp,
        })
    }
}

struct PatBuilder<'a>(&'a Renamer);

impl Build<Pat> for PatBuilder<'_> {
    fn fixity(&self, op: &QName) -> Fixity {
        self.0.fixity_of(op)
    }
    fn binary(&mut self, op: QName, l: Pat, r: Pat) -> Pat {
        let span = l.span.to(r.span);
        Pat {
            kind: PatKind::Con(op, vec![l, r]),
            span,
        }
    }
    fn negate(&mut self, span: Span, p: Pat) -> Result<Pat, ()> {
        match p.kind {
            PatKind::Lit(l @ (Lit::Int(_) | Lit::Frac(_))) => Ok(Pat {
                kind: PatKind::NegLit(l),
                span: span.to(p.span),
            }),
            _ => Err(()),
        }
    }
}

impl Renamer {
    /// The module defining the names wired into syntax (`negate`):
    /// `Cirka.Base` when the program has it, else `Prelude`.
    fn base_module(&self) -> &'static str {
        if self.module == BASE_MODULE || self.exports.contains_key(BASE_MODULE) {
            BASE_MODULE
        } else {
            "Prelude"
        }
    }

    pub fn new() -> Renamer {
        Renamer {
            errors: Vec::new(),
            exports: HashMap::new(),
            children: HashMap::new(),
            fixities: HashMap::new(),
            module: String::new(),
            scope: Scope::default(),
            locals: Vec::new(),
            local_fix: HashMap::new(),
            counter: 0,
            top_pat: false,
        }
    }

    fn error(&mut self, d: Diagnostic) {
        self.errors.push(d);
    }

    fn fixity_of(&self, op: &QName) -> Fixity {
        match &op.qual {
            None => self
                .local_fix
                .get(&op.name)
                .copied()
                .unwrap_or(fixity::DEFAULT),
            Some(m) => {
                if m == PRIM && op.name == ":" {
                    return Fixity {
                        assoc: Assoc::Right,
                        prec: 5,
                    };
                }
                self.fixities
                    .get(&(m.clone(), op.name.clone()))
                    .copied()
                    .unwrap_or(fixity::DEFAULT)
            }
        }
    }

    fn fresh(&mut self, name: &str) -> String {
        self.counter += 1;
        format!("{name}#{}", self.counter)
    }

    // ---- entities ----

    fn ent(&self, kind: Kind, name: &str, parent: Option<&str>) -> Ent {
        Ent {
            kind,
            module: self.module.clone(),
            name: name.to_string(),
            parent: parent.map(str::to_string),
        }
    }

    /// Top-level entities defined by the module.
    fn local_ents(&mut self, decls: &[Decl]) -> Vec<Ent> {
        let mut out: Vec<Ent> = Vec::new();
        let mut spans: HashMap<(Ns, String), Span> = HashMap::new();
        let mut last_fun: Option<String> = None;
        let mut add = |r: &mut Renamer, out: &mut Vec<Ent>, e: Ent, sp: Span| {
            let key = (e.kind.ns(), e.name.clone());
            if let Some(first) = spans.get(&key) {
                // Record fields shared by constructors are one entity.
                if e.kind == Kind::Field && out.contains(&e) {
                    return;
                }
                r.error(
                    err(
                        "E0203",
                        sp,
                        format!("multiple declarations of `{}`", e.name),
                    )
                    .with_secondary(*first, "first declared here"),
                );
                return;
            }
            spans.insert(key, sp);
            out.push(e);
        };
        for d in decls {
            match d {
                Decl::Fun(f, _, _, sp) => {
                    if last_fun.as_deref() == Some(&f.name) {
                        continue;
                    }
                    last_fun = Some(f.name.clone());
                    let e = self.ent(Kind::Var, &f.name, None);
                    add(self, &mut out, e, f.span.to(sp.to(f.span)));
                    continue;
                }
                Decl::PatBind(p, _, _) => {
                    let mut vs = Vec::new();
                    pat_vars(p, &mut vs);
                    for v in vs {
                        let e = self.ent(Kind::Var, &v.name, None);
                        add(self, &mut out, e, v.span);
                    }
                }
                Decl::Data(dd) => {
                    let t = self.ent(Kind::Type, &dd.name.name, None);
                    add(self, &mut out, t, dd.name.span);
                    for c in &dd.cons {
                        let e = self.ent(Kind::Con, &c.name.name, Some(&dd.name.name));
                        add(self, &mut out, e, c.name.span);
                        if let ConFields::Record(fs) = &c.fields {
                            for (ns, _, _) in fs {
                                for n in ns {
                                    let e = self.ent(Kind::Field, &n.name, Some(&dd.name.name));
                                    add(self, &mut out, e, n.span);
                                }
                            }
                        }
                    }
                }
                Decl::TypeSyn(n, _, _, _) => {
                    let e = self.ent(Kind::Type, &n.name, None);
                    add(self, &mut out, e, n.span);
                }
                Decl::Class(c) => {
                    let e = self.ent(Kind::Class, &c.name.name, None);
                    add(self, &mut out, e, c.name.span);
                    for d in &c.decls {
                        if let Decl::TypeSig(ns, _, _) = d {
                            for n in ns {
                                let e = self.ent(Kind::Method, &n.name, Some(&c.name.name));
                                add(self, &mut out, e, n.span);
                            }
                        }
                    }
                }
                Decl::Foreign(f) if f.import => {
                    let e = self.ent(Kind::Var, &f.name.name, None);
                    add(self, &mut out, e, f.name.span);
                }
                _ => {}
            }
            last_fun = None;
        }
        out
    }

    fn import_ents(&mut self, imp: &Import) -> Vec<Ent> {
        let Some(ex) = self.exports.get(&imp.module.name).cloned() else {
            return Vec::new();
        };
        let Some(items) = &imp.items else {
            return ex.ents;
        };
        let find = |r: &mut Renamer, ns: Ns, name: &Ident, var_only: bool| -> Vec<Ent> {
            let v: Vec<Ent> = ex
                .ents
                .iter()
                .filter(|e| {
                    e.kind.ns() == ns && e.name == name.name && !(var_only && e.kind == Kind::Con)
                })
                .cloned()
                .collect();
            if v.is_empty() && !imp.hiding {
                r.error(err(
                    "E0205",
                    name.span,
                    format!(
                        "module `{}` does not export `{}`",
                        imp.module.name, name.name
                    ),
                ));
            }
            v
        };
        let mut sel = Vec::new();
        for it in items {
            match it {
                ImportItem::Var(n) => sel.extend(find(self, Ns::Value, n, true)),
                ImportItem::Thing(n, sub) => {
                    let ts = find(self, Ns::Type, n, false);
                    for t in &ts {
                        let ch = ex.children(t);
                        match sub {
                            None => {}
                            Some(ItemList::All) => sel.extend(ch),
                            Some(ItemList::Some(names)) => {
                                for s in names {
                                    match ch.iter().find(|c| c.name == s.name) {
                                        Some(c) => sel.push(c.clone()),
                                        None => self.error(err(
                                            "E0205",
                                            s.span,
                                            format!("`{}` is not a part of `{}`", s.name, n.name),
                                        )),
                                    }
                                }
                            }
                        }
                    }
                    if imp.hiding {
                        // Hiding `C` also hides a data constructor `C`.
                        sel.extend(
                            ex.ents
                                .iter()
                                .filter(|e| e.kind == Kind::Con && e.name == n.name)
                                .cloned(),
                        );
                    }
                    sel.extend(ts);
                }
            }
        }
        if imp.hiding {
            ex.ents.into_iter().filter(|e| !sel.contains(e)).collect()
        } else {
            sel
        }
    }

    fn compute_exports(&mut self, m: &Module, locals: &[Ent]) -> Exports {
        let Some(list) = &m.exports else {
            return Exports {
                ents: locals.to_vec(),
            };
        };
        let mut out: Vec<Ent> = Vec::new();
        let push = |out: &mut Vec<Ent>, e: Ent| {
            if !out.contains(&e) {
                out.push(e);
            }
        };
        for x in list {
            match x {
                Export::Var(q) => {
                    if let Some(e) = self.lookup_ent(Ns::Value, q) {
                        push(&mut out, e);
                    }
                }
                Export::Thing(q, sub) => {
                    let Some(t) = self.lookup_ent(Ns::Type, q) else {
                        continue;
                    };
                    let ch = self
                        .children
                        .get(&(t.module.clone(), t.name.clone()))
                        .cloned()
                        .unwrap_or_default();
                    match sub {
                        None => {}
                        Some(ItemList::All) => {
                            for c in ch {
                                push(&mut out, c);
                            }
                        }
                        Some(ItemList::Some(ns)) => {
                            for n in ns {
                                match ch.iter().find(|c| c.name == n.name) {
                                    Some(c) => push(&mut out, c.clone()),
                                    None => self.error(err(
                                        "E0205",
                                        n.span,
                                        format!("`{}` is not a part of `{}`", n.name, q.name),
                                    )),
                                }
                            }
                        }
                    }
                    push(&mut out, t);
                }
                Export::Module(mn) => {
                    if mn.name == self.module {
                        for e in locals {
                            push(&mut out, e.clone());
                        }
                    } else {
                        let v = self.scope.via_module(&mn.name);
                        if v.is_empty() {
                            self.error(err(
                                "E0205",
                                mn.span,
                                format!("nothing is imported from module `{}`", mn.name),
                            ));
                        }
                        for e in v {
                            push(&mut out, e);
                        }
                    }
                }
            }
        }
        Exports { ents: out }
    }

    fn lookup_ent(&mut self, ns: Ns, q: &QName) -> Option<Ent> {
        let es = self.scope.lookup(ns, q.qual.as_deref(), &q.name).to_vec();
        match es.len() {
            0 => {
                let what = if ns == Ns::Type {
                    "type or class"
                } else {
                    "name"
                };
                self.error(err(
                    "E0201",
                    q.span,
                    format!("{what} `{}` is not in scope", q.text()),
                ));
                None
            }
            1 => Some(es[0].clone()),
            _ => {
                self.ambiguous(q, &es);
                None
            }
        }
    }

    fn ambiguous(&mut self, q: &QName, es: &[Ent]) {
        let mut d = err("E0202", q.span, format!("`{}` is ambiguous", q.text()));
        for e in es {
            d = d.with_note(format!("it could refer to `{}.{}`", e.module, e.name));
        }
        self.error(d.with_help("qualify the name or hide one of the imports"));
    }

    // ---- module ----

    pub fn module(&mut self, name: &str, m: Module) -> Module {
        self.module = name.to_string();
        self.scope = Scope::default();
        self.locals.clear();
        self.local_fix.clear();
        // Locals never leave their module: numbering per module keeps the
        // dumps stable.
        self.counter = 0;
        let locals = self.local_ents(&m.decls);
        for e in &locals {
            if let Some(p) = &e.parent {
                self.children
                    .entry((e.module.clone(), p.clone()))
                    .or_default()
                    .push(e.clone());
            }
            self.scope.add(None, e.clone());
            self.scope.add(Some(name.to_string()), e.clone());
        }
        for imp in &m.imports {
            let ents = self.import_ents(imp);
            let q = imp.alias.as_ref().unwrap_or(&imp.module).name.clone();
            for e in ents {
                if !imp.qualified {
                    self.scope.add(None, e.clone());
                }
                self.scope.add(Some(q.clone()), e);
            }
        }
        // Top-level fixity declarations.
        for d in &m.decls {
            if let Decl::Fixity(assoc, prec, ops, _) = d {
                for op in ops {
                    if !locals.iter().any(|e| e.name == op.name) {
                        self.error(err(
                            "E0201",
                            op.span,
                            format!(
                                "fixity declaration for `{}`, which is not defined here",
                                op.name
                            ),
                        ));
                    }
                    self.fixities.insert(
                        (name.to_string(), op.name.clone()),
                        Fixity {
                            assoc: *assoc,
                            prec: *prec,
                        },
                    );
                }
            }
        }
        // Class method fixities declared inside the class.
        for d in &m.decls {
            if let Decl::Class(c) = d {
                for cd in &c.decls {
                    if let Decl::Fixity(assoc, prec, ops, _) = cd {
                        for op in ops {
                            self.fixities.insert(
                                (name.to_string(), op.name.clone()),
                                Fixity {
                                    assoc: *assoc,
                                    prec: *prec,
                                },
                            );
                        }
                    }
                }
            }
        }
        let ex = self.compute_exports(&m, &locals);
        self.exports.insert(name.to_string(), ex);
        let top_binders: Vec<String> = locals
            .iter()
            .filter(|e| e.kind == Kind::Var)
            .map(|e| e.name.clone())
            .collect();
        self.check_sigs(&m.decls, &top_binders);
        let decls = m.decls.iter().map(|d| self.top_decl(d)).collect();
        Module {
            name: m.name,
            exports: m.exports,
            imports: m.imports,
            decls,
            extensions: m.extensions,
            span: m.span,
        }
    }

    /// Every type signature must have a binding in the same group.
    fn check_sigs(&mut self, decls: &[Decl], binders: &[String]) {
        for d in decls {
            if let Decl::TypeSig(ns, _, _) = d {
                for n in ns {
                    if !binders.contains(&n.name) {
                        self.error(err(
                            "E0606",
                            n.span,
                            format!(
                                "the type signature for `{}` lacks an accompanying binding",
                                n.name
                            ),
                        ));
                    }
                }
            }
        }
    }

    // ---- names ----

    fn resolve_value(&mut self, q: &QName) -> QName {
        if q.qual.is_none() {
            for scope in self.locals.iter().rev() {
                if let Some(u) = scope.get(&q.name) {
                    return QName {
                        qual: None,
                        name: u.clone(),
                        span: q.span,
                    };
                }
            }
            if builtin_value(&q.name) {
                return QName {
                    qual: Some(PRIM.into()),
                    name: q.name.clone(),
                    span: q.span,
                };
            }
        }
        match self.lookup_ent(Ns::Value, q) {
            Some(e) => QName {
                qual: Some(e.module),
                name: e.name,
                span: q.span,
            },
            None => q.clone(),
        }
    }

    fn resolve_type(&mut self, q: &QName) -> QName {
        if q.qual.is_none() && builtin_type(&q.name) {
            return QName {
                qual: Some(PRIM.into()),
                name: q.name.clone(),
                span: q.span,
            };
        }
        match self.lookup_ent(Ns::Type, q) {
            Some(e) => QName {
                qual: Some(e.module),
                name: e.name,
                span: q.span,
            },
            None => q.clone(),
        }
    }

    fn push_locals(&mut self, binds: &[(String, String)]) {
        let mut m = HashMap::new();
        for (orig, u) in binds {
            m.insert(orig.clone(), u.clone());
        }
        self.locals.push(m);
    }

    // ---- declarations ----

    fn top_decl(&mut self, d: &Decl) -> Decl {
        match d {
            Decl::TypeSig(ns, t, sp) => Decl::TypeSig(ns.clone(), self.ty(t), *sp),
            Decl::Fixity(..) | Decl::Default(..) => d.clone(),
            Decl::Fun(f, args, rhs, sp) => {
                let (args, rhs) = self.clause(args, rhs);
                Decl::Fun(f.clone(), args, rhs, *sp)
            }
            Decl::PatBind(p, rhs, sp) => {
                self.top_pat = true;
                let p = self.pat(p, &mut Vec::new());
                self.top_pat = false;
                let rhs = self.rhs(rhs);
                Decl::PatBind(p, rhs, *sp)
            }
            Decl::Data(dd) => {
                let context = dd.context.iter().map(|c| self.constraint(c)).collect();
                let cons = dd
                    .cons
                    .iter()
                    .map(|c| ConDecl {
                        name: c.name.clone(),
                        fields: match &c.fields {
                            ConFields::Plain(fs) => {
                                ConFields::Plain(fs.iter().map(|(t, s)| (self.ty(t), *s)).collect())
                            }
                            ConFields::Record(fs) => ConFields::Record(
                                fs.iter()
                                    .map(|(ns, t, s)| (ns.clone(), self.ty(t), *s))
                                    .collect(),
                            ),
                        },
                        span: c.span,
                    })
                    .collect();
                let deriving = dd.deriving.iter().map(|q| self.resolve_type(q)).collect();
                Decl::Data(DataDecl {
                    newtype: dd.newtype,
                    context,
                    name: dd.name.clone(),
                    params: dd.params.clone(),
                    param_kinds: dd.param_kinds.clone(),
                    cons,
                    deriving,
                    span: dd.span,
                })
            }
            Decl::TypeSyn(n, ps, t, sp) => Decl::TypeSyn(n.clone(), ps.clone(), self.ty(t), *sp),
            Decl::Class(c) => {
                let context = c.context.iter().map(|x| self.constraint(x)).collect();
                let decls = c.decls.iter().map(|d| self.top_decl(d)).collect();
                Decl::Class(ClassDecl {
                    context,
                    name: c.name.clone(),
                    params: c.params.clone(),
                    param_kinds: c.param_kinds.clone(),
                    decls,
                    span: c.span,
                })
            }
            Decl::Instance(i) => {
                let context = i.context.iter().map(|x| self.constraint(x)).collect();
                let class = self.resolve_type(&i.class);
                let types = i.types.iter().map(|t| self.ty(t)).collect();
                let methods: Vec<Ent> = self
                    .children
                    .get(&(class.qual.clone().unwrap_or_default(), class.name.clone()))
                    .cloned()
                    .unwrap_or_default();
                let mut decls = Vec::new();
                for d in &i.decls {
                    match d {
                        Decl::Fun(f, args, rhs, sp) => {
                            let Some(m) = methods.iter().find(|m| m.name == f.name) else {
                                self.error(err(
                                    "E0201",
                                    f.span,
                                    format!(
                                        "`{}` is not a method of class `{}`",
                                        f.name, class.name
                                    ),
                                ));
                                continue;
                            };
                            let (args, rhs) = self.clause(args, rhs);
                            let id = Ident {
                                name: format!("{}.{}", m.module, m.name),
                                span: f.span,
                            };
                            decls.push(Decl::Fun(id, args, rhs, *sp));
                        }
                        Decl::TypeSig(..) => self.error(err(
                            "E0102",
                            d.span(),
                            "type signatures are not allowed in instance declarations",
                        )),
                        other => decls.push(other.clone()),
                    }
                }
                Decl::Instance(InstDecl {
                    context,
                    class,
                    types,
                    decls,
                    span: i.span,
                })
            }
            Decl::Foreign(f) => {
                let mut f = f.clone();
                f.ty = self.ty(&f.ty);
                Decl::Foreign(f)
            }
        }
    }

    /// A binding group (`let` / `where`): binds its names, renames the
    /// declarations. The caller pops the scope.
    fn local_group(&mut self, decls: &[Decl]) -> Vec<Decl> {
        let mut binds: Vec<(String, String)> = Vec::new();
        let mut spans: HashMap<String, Span> = HashMap::new();
        let mut last_fun: Option<String> = None;
        for d in decls {
            let mut names = Vec::new();
            match d {
                Decl::Fun(f, _, _, _) => {
                    if last_fun.as_deref() != Some(&f.name) {
                        names.push(f.clone());
                    }
                    last_fun = Some(f.name.clone());
                }
                Decl::PatBind(p, _, _) => {
                    pat_vars(p, &mut names);
                    last_fun = None;
                }
                _ => last_fun = None,
            }
            for n in names {
                if let Some(first) = spans.get(&n.name) {
                    self.error(
                        err(
                            "E0203",
                            n.span,
                            format!("conflicting definitions for `{}`", n.name),
                        )
                        .with_secondary(*first, "first defined here"),
                    );
                    continue;
                }
                spans.insert(n.name.clone(), n.span);
                let u = self.fresh(&n.name);
                binds.push((n.name.clone(), u));
            }
        }
        let names: Vec<String> = binds.iter().map(|b| b.0.clone()).collect();
        self.check_sigs(decls, &names);
        self.push_locals(&binds);
        let uniq = |n: &str| binds.iter().find(|b| b.0 == n).map(|b| b.1.clone());
        let mut out = Vec::new();
        for d in decls {
            if let Decl::Fixity(a, p, ops, sp) = d {
                {
                    let mut ops2 = Vec::new();
                    for op in ops {
                        match uniq(&op.name) {
                            Some(u) => {
                                self.local_fix.insert(
                                    u.clone(),
                                    Fixity {
                                        assoc: *a,
                                        prec: *p,
                                    },
                                );
                                ops2.push(Ident {
                                    name: u,
                                    span: op.span,
                                });
                            }
                            None => self.error(err(
                                "E0201",
                                op.span,
                                format!(
                                    "fixity declaration for `{}`, which is not defined here",
                                    op.name
                                ),
                            )),
                        }
                    }
                    out.push(Decl::Fixity(*a, *p, ops2, *sp));
                }
            }
        }
        for d in decls {
            match d {
                Decl::TypeSig(ns, t, sp) => {
                    let ns = ns
                        .iter()
                        .map(|n| Ident {
                            name: uniq(&n.name).unwrap_or(n.name.clone()),
                            span: n.span,
                        })
                        .collect();
                    out.push(Decl::TypeSig(ns, self.ty(t), *sp));
                }
                Decl::Fun(f, args, rhs, sp) => {
                    let (args, rhs) = self.clause(args, rhs);
                    let f = Ident {
                        name: uniq(&f.name).unwrap_or(f.name.clone()),
                        span: f.span,
                    };
                    out.push(Decl::Fun(f, args, rhs, *sp));
                }
                Decl::PatBind(p, rhs, sp) => {
                    // The binders were already named; rename the pattern
                    // with the same unique names.
                    let p = self.pat_with(p, &binds);
                    let rhs = self.rhs(rhs);
                    out.push(Decl::PatBind(p, rhs, *sp));
                }
                Decl::Fixity(..) => {}
                other => {
                    self.error(err(
                        "E0102",
                        other.span(),
                        "this declaration is only allowed at the top level",
                    ));
                }
            }
        }
        out
    }

    /// Arguments and right-hand side of a function clause.
    fn clause(&mut self, args: &[Pat], rhs: &Rhs) -> (Vec<Pat>, Rhs) {
        let mut binds = Vec::new();
        let args: Vec<Pat> = args.iter().map(|p| self.pat(p, &mut binds)).collect();
        self.check_dup(&binds);
        self.push_locals(&strip(&binds));
        let rhs = self.rhs(rhs);
        self.locals.pop();
        (args, rhs)
    }

    fn check_dup(&mut self, binds: &[(String, String, Span)]) {
        for (i, (n, _, sp)) in binds.iter().enumerate() {
            if let Some((_, _, first)) = binds[..i].iter().find(|b| &b.0 == n) {
                self.error(
                    err("E0203", *sp, format!("conflicting definitions for `{n}`"))
                        .with_secondary(*first, "first bound here")
                        .with_note("a variable can appear only once in a pattern"),
                );
            }
        }
    }

    fn rhs(&mut self, r: &Rhs) -> Rhs {
        let wheres = if r.wheres.is_empty() {
            None
        } else {
            Some(self.local_group(&r.wheres))
        };
        let body = match &r.body {
            RhsBody::Plain(e) => RhsBody::Plain(self.expr(e)),
            RhsBody::Guarded(gs) => RhsBody::Guarded(gs.iter().map(|g| self.guarded(g)).collect()),
        };
        let wheres = match wheres {
            Some(w) => {
                self.locals.pop();
                w
            }
            None => Vec::new(),
        };
        Rhs { body, wheres }
    }

    fn guarded(&mut self, g: &GuardedExpr) -> GuardedExpr {
        let depth = self.locals.len();
        let guards = g.guards.iter().map(|s| self.stmt(s)).collect();
        let body = self.expr(&g.body);
        self.locals.truncate(depth);
        GuardedExpr {
            guards,
            body,
            span: g.span,
        }
    }

    /// A statement; bindings stay in scope (the caller truncates).
    fn stmt(&mut self, s: &Stmt) -> Stmt {
        match s {
            Stmt::Expr(e) => Stmt::Expr(self.expr(e)),
            Stmt::Bind(p, e, sp) => {
                let e = self.expr(e);
                let mut binds = Vec::new();
                let p = self.pat(p, &mut binds);
                self.check_dup(&binds);
                self.push_locals(&strip(&binds));
                Stmt::Bind(p, e, *sp)
            }
            Stmt::Let(ds, sp) => Stmt::Let(self.local_group(ds), *sp),
        }
    }

    // ---- expressions ----

    fn expr(&mut self, e: &Expr) -> Expr {
        let span = e.span;
        let kind = match &e.kind {
            ExprKind::Var(q) => ExprKind::Var(self.resolve_value(q)),
            ExprKind::Con(q) => ExprKind::Con(self.resolve_value(q)),
            ExprKind::Lit(l) => ExprKind::Lit(l.clone()),
            ExprKind::App(f, a) => ExprKind::App(Box::new(self.expr(f)), Box::new(self.expr(a))),
            ExprKind::OpSeq(items) => {
                let items: Vec<OpItem<Expr>> = items
                    .iter()
                    .map(|i| match i {
                        OpItem::Operand(x) => OpItem::Operand(self.expr(x)),
                        OpItem::Op(q) => OpItem::Op(self.resolve_value(q)),
                        OpItem::Neg(s) => OpItem::Neg(*s),
                    })
                    .collect();
                let r = fixity::resolve(items, &mut ExprBuilder(self));
                match r {
                    Ok(e) => return e,
                    Err(c) => {
                        self.conflict(c, span);
                        ExprKind::Error
                    }
                }
            }
            ExprKind::Lam(ps, body) => {
                let mut binds = Vec::new();
                let ps = ps.iter().map(|p| self.pat(p, &mut binds)).collect();
                self.check_dup(&binds);
                self.push_locals(&strip(&binds));
                let b = self.expr(body);
                self.locals.pop();
                ExprKind::Lam(ps, Box::new(b))
            }
            ExprKind::Let(ds, body) => {
                let ds = self.local_group(ds);
                let b = self.expr(body);
                self.locals.pop();
                ExprKind::Let(ds, Box::new(b))
            }
            ExprKind::If(c, t, f) => ExprKind::If(
                Box::new(self.expr(c)),
                Box::new(self.expr(t)),
                Box::new(self.expr(f)),
            ),
            ExprKind::MultiIf(gs) => {
                ExprKind::MultiIf(gs.iter().map(|g| self.guarded(g)).collect())
            }
            ExprKind::Case(s, alts) => {
                let s = self.expr(s);
                ExprKind::Case(Box::new(s), self.alts(alts))
            }
            ExprKind::LamCase(alts) => ExprKind::LamCase(self.alts(alts)),
            ExprKind::Do(ss) => {
                if let Some(last) = ss.last()
                    && !matches!(last, Stmt::Expr(_))
                {
                    let sp = match last {
                        Stmt::Bind(_, _, s) | Stmt::Let(_, s) => *s,
                        Stmt::Expr(e) => e.span,
                    };
                    self.error(err(
                        "E0102",
                        sp,
                        "the last statement in a `do` block must be an expression",
                    ));
                }
                let depth = self.locals.len();
                let ss = ss.iter().map(|s| self.stmt(s)).collect();
                self.locals.truncate(depth);
                ExprKind::Do(ss)
            }
            ExprKind::Tuple(es) => ExprKind::Tuple(es.iter().map(|e| self.expr(e)).collect()),
            ExprKind::TupleSection(es) => ExprKind::TupleSection(
                es.iter()
                    .map(|e| e.as_ref().map(|e| self.expr(e)))
                    .collect(),
            ),
            ExprKind::List(es) => ExprKind::List(es.iter().map(|e| self.expr(e)).collect()),
            ExprKind::Range(a, b, c) => ExprKind::Range(
                Box::new(self.expr(a)),
                b.as_ref().map(|e| Box::new(self.expr(e))),
                c.as_ref().map(|e| Box::new(self.expr(e))),
            ),
            ExprKind::ListComp(h, qs) => {
                let depth = self.locals.len();
                let qs = qs.iter().map(|s| self.stmt(s)).collect();
                let h = self.expr(h);
                self.locals.truncate(depth);
                ExprKind::ListComp(Box::new(h), qs)
            }
            ExprKind::LeftSection(e, op) => {
                ExprKind::LeftSection(Box::new(self.expr(e)), self.resolve_value(op))
            }
            ExprKind::RightSection(op, e) => {
                ExprKind::RightSection(self.resolve_value(op), Box::new(self.expr(e)))
            }
            ExprKind::Typed(e, t) => ExprKind::Typed(Box::new(self.expr(e)), self.ty(t)),
            ExprKind::RecordCon(c, fs) => {
                let c = self.resolve_value(c);
                let fs = fs
                    .iter()
                    .map(|(f, v)| {
                        let fq = self.resolve_value(&QName {
                            qual: f.qual.clone(),
                            name: f.name.clone(),
                            span: f.span,
                        });
                        let v = match v {
                            Some(v) => self.expr(v),
                            // Pun: `C { f }` is `C { f = f }`.
                            None => Expr {
                                kind: ExprKind::Var(
                                    self.resolve_value(&QName::unqual(f.name.clone(), f.span)),
                                ),
                                span: f.span,
                            },
                        };
                        (fq, Some(v))
                    })
                    .collect();
                ExprKind::RecordCon(c, fs)
            }
            ExprKind::RecordUpd(e, fs) => {
                let e = self.expr(e);
                let fs = fs
                    .iter()
                    .map(|(f, v)| (self.resolve_value(f), self.expr(v)))
                    .collect();
                ExprKind::RecordUpd(Box::new(e), fs)
            }
            ExprKind::Paren(e) => return self.expr(e),
            ExprKind::Error => ExprKind::Error,
        };
        Expr { kind, span }
    }

    fn alts(&mut self, alts: &[Alt]) -> Vec<Alt> {
        alts.iter()
            .map(|a| {
                let mut binds = Vec::new();
                let pat = self.pat(&a.pat, &mut binds);
                self.check_dup(&binds);
                self.push_locals(&strip(&binds));
                let rhs = self.rhs(&a.rhs);
                self.locals.pop();
                Alt {
                    pat,
                    rhs,
                    span: a.span,
                }
            })
            .collect()
    }

    fn conflict(&mut self, c: Conflict, span: Span) {
        let d = match c {
            Conflict::Ops(a, b) => {
                let fa = self.fixity_of(&a);
                let fb = self.fixity_of(&b);
                let show = |f: Fixity| {
                    let a = match f.assoc {
                        Assoc::Left => "infixl",
                        Assoc::Right => "infixr",
                        Assoc::None => "infix",
                    };
                    format!("{a} {}", f.prec)
                };
                err(
                    "E0204",
                    b.span,
                    format!(
                        "cannot mix `{}` [{}] and `{}` [{}] in the same infix expression",
                        display_name(&a.name),
                        show(fa),
                        display_name(&b.name),
                        show(fb)
                    ),
                )
                .with_help("add parentheses")
            }
            Conflict::Neg(sp, op) => err(
                "E0204",
                sp,
                format!(
                    "cannot use prefix `-` after `{}` (precedence {})",
                    display_name(&op.name),
                    self.fixity_of(&op).prec
                ),
            )
            .with_help("put the negated expression in parentheses"),
            Conflict::Other => err("E0102", span, "invalid operator expression"),
        };
        self.error(d);
    }

    // ---- patterns ----

    fn pat(&mut self, p: &Pat, binds: &mut Vec<(String, String, Span)>) -> Pat {
        let span = p.span;
        let kind = match &p.kind {
            PatKind::Var(v) => PatKind::Var(self.bind_var(v, binds)),
            PatKind::Wild => PatKind::Wild,
            PatKind::Lit(l) => PatKind::Lit(l.clone()),
            PatKind::NegLit(l) => PatKind::NegLit(l.clone()),
            PatKind::Con(c, ps) => {
                let c = self.resolve_value(c);
                PatKind::Con(c, ps.iter().map(|p| self.pat(p, binds)).collect())
            }
            PatKind::OpSeq(items) => {
                if let Some(op) = items.iter().find_map(|i| match i {
                    OpItem::Op(q) if !q.is_con() => Some(q.clone()),
                    _ => None,
                }) {
                    self.error(
                        err(
                            "E0102",
                            op.span,
                            format!("`{}` is not a constructor operator", op.text()),
                        )
                        .with_note("only constructor operators can appear in patterns"),
                    );
                    // Bind the variables anyway to avoid follow-up errors.
                    for i in items {
                        if let OpItem::Operand(x) = i {
                            self.pat(x, binds);
                        }
                    }
                    return Pat {
                        kind: PatKind::Error,
                        span,
                    };
                }
                let items: Vec<OpItem<Pat>> = items
                    .iter()
                    .map(|i| match i {
                        OpItem::Operand(x) => OpItem::Operand(self.pat(x, binds)),
                        OpItem::Op(q) => OpItem::Op(self.resolve_value(q)),
                        OpItem::Neg(s) => OpItem::Neg(*s),
                    })
                    .collect();
                match fixity::resolve(items, &mut PatBuilder(self)) {
                    Ok(p) => return p,
                    Err(c) => {
                        self.conflict(c, span);
                        PatKind::Error
                    }
                }
            }
            PatKind::Tuple(ps) => PatKind::Tuple(ps.iter().map(|p| self.pat(p, binds)).collect()),
            PatKind::List(ps) => PatKind::List(ps.iter().map(|p| self.pat(p, binds)).collect()),
            PatKind::As(v, p) => {
                let v = self.bind_var(v, binds);
                PatKind::As(v, Box::new(self.pat(p, binds)))
            }
            PatKind::Lazy(p) => PatKind::Lazy(Box::new(self.pat(p, binds))),
            PatKind::Bang(p) => PatKind::Bang(Box::new(self.pat(p, binds))),
            PatKind::Record(c, fs) => {
                let c = self.resolve_value(c);
                let fs = fs
                    .iter()
                    .map(|(f, p)| {
                        let fq = self.resolve_value(f);
                        let p = match p {
                            Some(p) => self.pat(p, binds),
                            None => {
                                let id = Ident {
                                    name: f.name.clone(),
                                    span: f.span,
                                };
                                Pat {
                                    kind: PatKind::Var(self.bind_var(&id, binds)),
                                    span: f.span,
                                }
                            }
                        };
                        (fq, Some(p))
                    })
                    .collect();
                PatKind::Record(c, fs)
            }
            PatKind::Paren(p) => return self.pat(p, binds),
            PatKind::Typed(p, t) => {
                let p = self.pat(p, binds);
                PatKind::Typed(Box::new(p), self.ty(t))
            }
            PatKind::Error => PatKind::Error,
        };
        Pat { kind, span }
    }

    fn bind_var(&mut self, v: &Ident, binds: &mut Vec<(String, String, Span)>) -> Ident {
        if self.top_pat {
            return v.clone();
        }
        let u = self.fresh(&v.name);
        binds.push((v.name.clone(), u.clone(), v.span));
        Ident {
            name: u,
            span: v.span,
        }
    }

    /// Renames a pattern whose variables were already given unique names.
    fn pat_with(&mut self, p: &Pat, names: &[(String, String)]) -> Pat {
        let mut binds = Vec::new();
        let mut q = self.pat(p, &mut binds);
        let map: HashMap<String, String> = binds
            .iter()
            .filter_map(|(orig, fresh, _)| {
                names
                    .iter()
                    .find(|n| &n.0 == orig)
                    .map(|n| (fresh.clone(), n.1.clone()))
            })
            .collect();
        rename_pat_vars(&mut q, &map);
        q
    }

    // ---- types ----

    fn constraint(&mut self, c: &Constraint) -> Constraint {
        Constraint {
            class: self.resolve_type(&c.class),
            types: c.types.iter().map(|t| self.ty(t)).collect(),
        }
    }

    fn ty(&mut self, t: &Type) -> Type {
        let kind = match &t.kind {
            TypeKind::Var(v) => TypeKind::Var(v.clone()),
            TypeKind::Con(q) => TypeKind::Con(self.resolve_type(q)),
            TypeKind::App(f, a) => TypeKind::App(Box::new(self.ty(f)), Box::new(self.ty(a))),
            TypeKind::Fun(a, b) => TypeKind::Fun(Box::new(self.ty(a)), Box::new(self.ty(b))),
            TypeKind::List(a) => TypeKind::List(Box::new(self.ty(a))),
            TypeKind::Tuple(ts) => TypeKind::Tuple(ts.iter().map(|t| self.ty(t)).collect()),
            TypeKind::Qual(ctx, b) => TypeKind::Qual(
                ctx.iter().map(|c| self.constraint(c)).collect(),
                Box::new(self.ty(b)),
            ),
            TypeKind::Forall(vs, b) => TypeKind::Forall(vs.clone(), Box::new(self.ty(b))),
            TypeKind::Kinded(b, k) => TypeKind::Kinded(Box::new(self.ty(b)), k.clone()),
            TypeKind::MulFun(m, a, b) => {
                TypeKind::MulFun(m.clone(), Box::new(self.ty(a)), Box::new(self.ty(b)))
            }
        };
        Type { kind, span: t.span }
    }
}

fn strip(binds: &[(String, String, Span)]) -> Vec<(String, String)> {
    binds
        .iter()
        .map(|(a, b, _)| (a.clone(), b.clone()))
        .collect()
}

fn rename_pat_vars(p: &mut Pat, map: &HashMap<String, String>) {
    match &mut p.kind {
        PatKind::Var(v) => {
            if let Some(n) = map.get(&v.name) {
                v.name = n.clone();
            }
        }
        PatKind::As(v, q) => {
            if let Some(n) = map.get(&v.name) {
                v.name = n.clone();
            }
            rename_pat_vars(q, map);
        }
        PatKind::Con(_, ps) | PatKind::Tuple(ps) | PatKind::List(ps) => {
            for q in ps {
                rename_pat_vars(q, map);
            }
        }
        PatKind::Lazy(q) | PatKind::Bang(q) | PatKind::Paren(q) | PatKind::Typed(q, _) => {
            rename_pat_vars(q, map)
        }
        PatKind::Record(_, fs) => {
            for (_, q) in fs.iter_mut() {
                if let Some(q) = q {
                    rename_pat_vars(q, map);
                }
            }
        }
        PatKind::OpSeq(items) => {
            for i in items {
                if let OpItem::Operand(q) = i {
                    rename_pat_vars(q, map);
                }
            }
        }
        _ => {}
    }
}

/// Variables bound by a pattern (in order).
pub fn pat_vars(p: &Pat, out: &mut Vec<Ident>) {
    match &p.kind {
        PatKind::Var(v) => out.push(v.clone()),
        PatKind::As(v, q) => {
            out.push(v.clone());
            pat_vars(q, out);
        }
        PatKind::Con(_, ps) | PatKind::Tuple(ps) | PatKind::List(ps) => {
            for q in ps {
                pat_vars(q, out);
            }
        }
        PatKind::Lazy(q) | PatKind::Bang(q) | PatKind::Paren(q) | PatKind::Typed(q, _) => {
            pat_vars(q, out)
        }
        PatKind::Record(_, fs) => {
            for (f, q) in fs {
                match q {
                    Some(q) => pat_vars(q, out),
                    None => out.push(Ident {
                        name: f.name.clone(),
                        span: f.span,
                    }),
                }
            }
        }
        PatKind::OpSeq(items) => {
            for i in items {
                if let OpItem::Operand(q) = i {
                    pat_vars(q, out);
                }
            }
        }
        _ => {}
    }
}
