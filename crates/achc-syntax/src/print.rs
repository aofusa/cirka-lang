//! S-expression dump of the AST (`--dump=ast` and snapshot tests).

use crate::ast::*;
use std::fmt::Write;

pub fn print_module(m: &Module) -> String {
    let mut p = P::default();
    p.line(&format!(
        "(module {}",
        m.name.as_ref().map(|n| n.name.as_str()).unwrap_or("Main")
    ));
    p.depth += 1;
    for (e, _) in &m.extensions {
        p.line(&format!("(language {e})"));
    }
    if let Some(es) = &m.exports {
        let v: Vec<String> = es.iter().map(export).collect();
        p.line(&format!("(exports {})", v.join(" ")));
    }
    for i in &m.imports {
        p.line(&import(i));
    }
    for d in &m.decls {
        p.decl(d);
    }
    p.depth -= 1;
    p.line(")");
    p.out
}

fn items(l: &Option<ItemList>) -> String {
    match l {
        None => String::new(),
        Some(ItemList::All) => "(..)".into(),
        Some(ItemList::Some(v)) => {
            let n: Vec<&str> = v.iter().map(|i| i.name.as_str()).collect();
            format!("({})", n.join(" "))
        }
    }
}

fn export(e: &Export) -> String {
    match e {
        Export::Var(q) => q.text(),
        Export::Thing(q, l) => format!("{}{}", q.text(), items(l)),
        Export::Module(m) => format!("(module {})", m.name),
    }
}

fn import(i: &Import) -> String {
    let mut s = format!("(import {}", i.module.name);
    if i.qualified {
        s.push_str(" qualified");
    }
    if let Some(a) = &i.alias {
        let _ = write!(s, " as {}", a.name);
    }
    if i.hiding {
        s.push_str(" hiding");
    }
    if let Some(its) = &i.items {
        let v: Vec<String> = its
            .iter()
            .map(|it| match it {
                ImportItem::Var(n) => n.name.clone(),
                ImportItem::Thing(n, l) => format!("{}{}", n.name, items(l)),
            })
            .collect();
        let _ = write!(s, " ({})", v.join(" "));
    }
    s.push(')');
    s
}

pub fn ty(t: &Type) -> String {
    match &t.kind {
        TypeKind::Var(v) => v.clone(),
        TypeKind::Con(c) => c.text(),
        TypeKind::App(f, a) => format!("({} {})", ty(f), ty(a)),
        TypeKind::Fun(a, b) => format!("(-> {} {})", ty(a), ty(b)),
        TypeKind::List(a) => format!("[{}]", ty(a)),
        TypeKind::Tuple(ts) => {
            let v: Vec<String> = ts.iter().map(ty).collect();
            format!("(tuple {})", v.join(" "))
        }
        TypeKind::Qual(ctx, b) => format!("(=> ({}) {})", context(ctx), ty(b)),
        TypeKind::Forall(vs, b) => {
            let v: Vec<&str> = vs.iter().map(|v| v.name.as_str()).collect();
            format!("(forall ({}) {})", v.join(" "), ty(b))
        }
    }
}

fn context(ctx: &[Constraint]) -> String {
    let v: Vec<String> = ctx
        .iter()
        .map(|c| {
            let ts: Vec<String> = c.types.iter().map(ty).collect();
            format!("({} {})", c.class.text(), ts.join(" "))
        })
        .collect();
    v.join(" ")
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Frac(x) => format!("{x:?}"),
        Lit::Char(c) => format!("{c:?}"),
        Lit::Str(s) => format!("{s:?}"),
    }
}

fn seq<T>(items: &[OpItem<T>], f: impl Fn(&T) -> String) -> String {
    let v: Vec<String> = items
        .iter()
        .map(|i| match i {
            OpItem::Operand(x) => f(x),
            OpItem::Op(q) => format!("`{}`", q.text()),
            OpItem::Neg(_) => "neg".into(),
        })
        .collect();
    format!("(ops {})", v.join(" "))
}

pub fn pat(p: &Pat) -> String {
    match &p.kind {
        PatKind::Var(v) => v.name.clone(),
        PatKind::Wild => "_".into(),
        PatKind::Lit(l) => lit(l),
        PatKind::NegLit(l) => format!("-{}", lit(l)),
        PatKind::Con(c, ps) if ps.is_empty() => c.text(),
        PatKind::Con(c, ps) => {
            let v: Vec<String> = ps.iter().map(pat).collect();
            format!("({} {})", c.text(), v.join(" "))
        }
        PatKind::OpSeq(items) => seq(items, pat),
        PatKind::Tuple(ps) => {
            let v: Vec<String> = ps.iter().map(pat).collect();
            format!("(tuple {})", v.join(" "))
        }
        PatKind::List(ps) => {
            let v: Vec<String> = ps.iter().map(pat).collect();
            format!("[{}]", v.join(" "))
        }
        PatKind::As(x, p) => format!("{}@{}", x.name, pat(p)),
        PatKind::Lazy(p) => format!("~{}", pat(p)),
        PatKind::Bang(p) => format!("!{}", pat(p)),
        PatKind::Record(c, fs) => {
            let v: Vec<String> = fs
                .iter()
                .map(|(f, p)| match p {
                    Some(p) => format!("({} {})", f.text(), pat(p)),
                    None => f.text(),
                })
                .collect();
            format!("(record {} {})", c.text(), v.join(" "))
        }
        PatKind::Paren(p) => pat(p),
        PatKind::Typed(p, t) => format!("(:: {} {})", pat(p), ty(t)),
        PatKind::Error => "<error>".into(),
    }
}

#[derive(Default)]
struct P {
    out: String,
    depth: usize,
}

impl P {
    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn decl(&mut self, d: &Decl) {
        match d {
            Decl::TypeSig(ns, t, _) => {
                let v: Vec<&str> = ns.iter().map(|n| n.name.as_str()).collect();
                self.line(&format!("(sig ({}) {})", v.join(" "), ty(t)));
            }
            Decl::Fixity(a, p, ops, _) => {
                let a = match a {
                    Assoc::Left => "infixl",
                    Assoc::Right => "infixr",
                    Assoc::None => "infix",
                };
                let v: Vec<&str> = ops.iter().map(|n| n.name.as_str()).collect();
                self.line(&format!("({a} {p} {})", v.join(" ")));
            }
            Decl::Fun(f, ps, rhs, _) => {
                let v: Vec<String> = ps.iter().map(pat).collect();
                self.line(&format!("(fun {} ({})", f.name, v.join(" ")));
                self.rhs(rhs);
            }
            Decl::PatBind(p, rhs, _) => {
                self.line(&format!("(patbind {}", pat(p)));
                self.rhs(rhs);
            }
            Decl::Data(dd) => {
                let ps: Vec<&str> = dd.params.iter().map(|n| n.name.as_str()).collect();
                let kw = if dd.newtype { "newtype" } else { "data" };
                let ctx = if dd.context.is_empty() {
                    String::new()
                } else {
                    format!(" (context {})", context(&dd.context))
                };
                self.line(&format!("({kw}{ctx} {} ({})", dd.name.name, ps.join(" ")));
                self.depth += 1;
                for c in &dd.cons {
                    let fs = match &c.fields {
                        ConFields::Plain(fs) => {
                            let v: Vec<String> = fs
                                .iter()
                                .map(|(t, s)| format!("{}{}", if *s { "!" } else { "" }, ty(t)))
                                .collect();
                            v.join(" ")
                        }
                        ConFields::Record(fs) => {
                            let v: Vec<String> = fs
                                .iter()
                                .map(|(ns, t, s)| {
                                    let n: Vec<&str> = ns.iter().map(|n| n.name.as_str()).collect();
                                    format!(
                                        "({} {}{})",
                                        n.join(" "),
                                        if *s { "!" } else { "" },
                                        ty(t)
                                    )
                                })
                                .collect();
                            format!("(record {})", v.join(" "))
                        }
                    };
                    self.line(&format!("(con {} {fs})", c.name.name));
                }
                if !dd.deriving.is_empty() {
                    let v: Vec<String> = dd.deriving.iter().map(QName::text).collect();
                    self.line(&format!("(deriving {})", v.join(" ")));
                }
                self.depth -= 1;
                self.line(")");
            }
            Decl::TypeSyn(n, ps, t, _) => {
                let v: Vec<&str> = ps.iter().map(|n| n.name.as_str()).collect();
                self.line(&format!("(type {} ({}) {})", n.name, v.join(" "), ty(t)));
            }
            Decl::Class(c) => {
                let ps: Vec<&str> = c.params.iter().map(|n| n.name.as_str()).collect();
                self.line(&format!(
                    "(class ({}) {} ({})",
                    context(&c.context),
                    c.name.name,
                    ps.join(" ")
                ));
                self.depth += 1;
                for d in &c.decls {
                    self.decl(d);
                }
                self.depth -= 1;
                self.line(")");
            }
            Decl::Instance(i) => {
                let ts: Vec<String> = i.types.iter().map(ty).collect();
                self.line(&format!(
                    "(instance ({}) {} {}",
                    context(&i.context),
                    i.class.text(),
                    ts.join(" ")
                ));
                self.depth += 1;
                for d in &i.decls {
                    self.decl(d);
                }
                self.depth -= 1;
                self.line(")");
            }
            Decl::Default(ts, _) => {
                let v: Vec<String> = ts.iter().map(ty).collect();
                self.line(&format!("(default {})", v.join(" ")));
            }
            Decl::Foreign(f) => {
                self.line(&format!(
                    "(foreign {} {} {:?} {} {})",
                    if f.import { "import" } else { "export" },
                    f.callconv.name,
                    f.entity.as_deref().unwrap_or(""),
                    f.name.name,
                    ty(&f.ty)
                ));
            }
        }
    }

    fn rhs(&mut self, r: &Rhs) {
        self.depth += 1;
        match &r.body {
            RhsBody::Plain(e) => self.line(&expr(e)),
            RhsBody::Guarded(gs) => {
                for g in gs {
                    self.line(&guarded(g));
                }
            }
        }
        if !r.wheres.is_empty() {
            self.line("(where");
            self.depth += 1;
            for d in &r.wheres {
                self.decl(d);
            }
            self.depth -= 1;
            self.line(")");
        }
        self.depth -= 1;
        self.line(")");
    }
}

fn guarded(g: &GuardedExpr) -> String {
    let v: Vec<String> = g.guards.iter().map(stmt).collect();
    format!("(| ({}) {})", v.join(" "), expr(&g.body))
}

fn stmt(s: &Stmt) -> String {
    match s {
        Stmt::Bind(p, e, _) => format!("(<- {} {})", pat(p), expr(e)),
        Stmt::Let(ds, _) => format!("(let {})", decls_inline(ds)),
        Stmt::Expr(e) => expr(e),
    }
}

fn decls_inline(ds: &[Decl]) -> String {
    let mut p = P::default();
    for d in ds {
        p.decl(d);
    }
    let lines: Vec<&str> = p.out.lines().map(str::trim).collect();
    lines.join(" ")
}

fn rhs_inline(r: &Rhs) -> String {
    let mut p = P::default();
    p.rhs(r);
    let lines: Vec<&str> = p.out.lines().map(str::trim).collect();
    let s = lines.join(" ");
    s.strip_suffix(" )").unwrap_or(&s).to_string()
}

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Var(q) | ExprKind::Con(q) => q.text(),
        ExprKind::Lit(l) => lit(l),
        ExprKind::App(f, a) => format!("({} {})", expr(f), expr(a)),
        ExprKind::OpSeq(items) => seq(items, expr),
        ExprKind::Lam(ps, b) => {
            let v: Vec<String> = ps.iter().map(pat).collect();
            format!("(\\ ({}) {})", v.join(" "), expr(b))
        }
        ExprKind::Let(ds, b) => format!("(let ({}) {})", decls_inline(ds), expr(b)),
        ExprKind::If(c, t, f) => format!("(if {} {} {})", expr(c), expr(t), expr(f)),
        ExprKind::MultiIf(gs) => {
            let v: Vec<String> = gs.iter().map(guarded).collect();
            format!("(if| {})", v.join(" "))
        }
        ExprKind::Case(s, alts) => format!("(case {} {})", expr(s), alts_str(alts)),
        ExprKind::LamCase(alts) => format!("(\\case {})", alts_str(alts)),
        ExprKind::Do(ss) => {
            let v: Vec<String> = ss.iter().map(stmt).collect();
            format!("(do {})", v.join(" "))
        }
        ExprKind::Tuple(es) => {
            let v: Vec<String> = es.iter().map(expr).collect();
            format!("(tuple {})", v.join(" "))
        }
        ExprKind::TupleSection(es) => {
            let v: Vec<String> = es
                .iter()
                .map(|e| e.as_ref().map(expr).unwrap_or("_".into()))
                .collect();
            format!("(tuple-section {})", v.join(" "))
        }
        ExprKind::List(es) => {
            let v: Vec<String> = es.iter().map(expr).collect();
            format!("[{}]", v.join(" "))
        }
        ExprKind::Range(a, b, c) => format!(
            "(range {} {} {})",
            expr(a),
            b.as_ref().map(|e| expr(e)).unwrap_or("_".into()),
            c.as_ref().map(|e| expr(e)).unwrap_or("_".into())
        ),
        ExprKind::ListComp(e, qs) => {
            let v: Vec<String> = qs.iter().map(stmt).collect();
            format!("(listcomp {} {})", expr(e), v.join(" "))
        }
        ExprKind::LeftSection(e, op) => format!("(section {} `{}`)", expr(e), op.text()),
        ExprKind::RightSection(op, e) => format!("(section `{}` {})", op.text(), expr(e)),
        ExprKind::Typed(e, t) => format!("(:: {} {})", expr(e), ty(t)),
        ExprKind::RecordCon(c, fs) => {
            let v: Vec<String> = fs
                .iter()
                .map(|(f, e)| match e {
                    Some(e) => format!("({} {})", f.text(), expr(e)),
                    None => f.text(),
                })
                .collect();
            format!("(record {} {})", c.text(), v.join(" "))
        }
        ExprKind::RecordUpd(e, fs) => {
            let v: Vec<String> = fs
                .iter()
                .map(|(f, e)| format!("({} {})", f.text(), expr(e)))
                .collect();
            format!("(update {} {})", expr(e), v.join(" "))
        }
        ExprKind::Paren(e) => format!("(paren {})", expr(e)),
        ExprKind::Error => "<error>".into(),
    }
}

fn alts_str(alts: &[Alt]) -> String {
    let v: Vec<String> = alts
        .iter()
        .map(|a| format!("({} -> {})", pat(&a.pat), rhs_inline(&a.rhs)))
        .collect();
    v.join(" ")
}
