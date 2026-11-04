//! Pretty printer for the Core text format. The output parses back to an
//! equivalent program: variables whose names are ambiguous within a
//! definition are printed as `name~id`.

use crate::expr::*;
use crate::sexp::escape;
use crate::ty::*;
use std::collections::HashMap;
use std::fmt::Write;

pub fn print_program(p: &Program) -> String {
    let mut s = String::new();
    for d in &p.datas {
        let mut names = Names::default();
        for (v, _) in &d.params {
            names.tv(v);
        }
        let _ = write!(s, "(data {} (", d.name);
        for (i, (v, k)) in d.params.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            if *k == Kind::Type {
                s.push_str(&names.tvname(v));
            } else {
                let _ = write!(s, "({} {k})", names.tvname(v));
            }
        }
        s.push(')');
        if d.linear {
            s.push_str(" :linear");
        }
        if d.newtype {
            s.push_str(" :newtype");
        }
        for c in &d.ctors {
            let _ = write!(s, " ({}", c.name);
            for (f, st) in c.fields.iter().zip(&c.strict) {
                if *st {
                    let _ = write!(s, " (! {})", names.ty(f));
                } else {
                    let _ = write!(s, " {}", names.ty(f));
                }
            }
            s.push(')');
        }
        s.push_str(")\n");
    }
    for e in &p.externs {
        let names = Names::default();
        let ps: Vec<String> = e.params.iter().map(|t| names.ty(t)).collect();
        let _ = writeln!(
            s,
            "(extern {} ({}) {})",
            e.name,
            ps.join(" "),
            names.ty(&e.ret)
        );
    }
    for b in &p.binds {
        s.push_str(&print_bind(b));
    }
    s
}

pub fn print_bind(b: &TopBind) -> String {
    let mut names = Names::default();
    names.collect_ty(&b.ty);
    names.collect(&b.expr);
    let mut out = String::new();
    let _ = write!(out, "(def {} {}\n  ", b.name, names.ty(&b.ty));
    names.expr(&b.expr, 1, &mut out);
    out.push_str(")\n");
    out
}

/// Prints a single expression (variables named `name~id` when ambiguous).
pub fn print_expr(e: &Expr) -> String {
    let mut names = Names::default();
    names.collect(e);
    let mut out = String::new();
    names.expr(e, 0, &mut out);
    out
}

#[derive(Default)]
struct Names {
    /// name -> distinct ids seen
    vars: HashMap<String, Vec<u32>>,
    tvs: HashMap<String, Vec<u32>>,
}

impl Names {
    fn var(&mut self, i: &Id) {
        let e = self.vars.entry(i.name.clone()).or_default();
        if !e.contains(&i.id) {
            e.push(i.id);
        }
    }

    fn tv(&mut self, v: &TyVar) {
        let e = self.tvs.entry(v.name.clone()).or_default();
        if !e.contains(&v.id) {
            e.push(v.id);
        }
    }

    fn vname(&self, i: &Id) -> String {
        match self.vars.get(&i.name) {
            Some(ids) if ids.len() > 1 => format!("{}~{}", i.name, i.id),
            _ => i.name.clone(),
        }
    }

    fn tvname(&self, v: &TyVar) -> String {
        match self.tvs.get(&v.name) {
            Some(ids) if ids.len() > 1 => format!("{}~{}", v.name, v.id),
            _ => v.name.clone(),
        }
    }

    fn collect_ty(&mut self, t: &Type) {
        match t {
            Type::Var(v) => self.tv(v),
            Type::Con(_, a) => a.iter().for_each(|x| self.collect_ty(x)),
            Type::App(h, a) => {
                self.collect_ty(h);
                a.iter().for_each(|x| self.collect_ty(x));
            }
            Type::Fun(_, a, b) => {
                self.collect_ty(a);
                self.collect_ty(b);
            }
            Type::Forall(v, _, b) => {
                self.tv(v);
                self.collect_ty(b);
            }
            Type::Any => {}
        }
    }

    fn collect_binder(&mut self, b: &Binder) {
        self.var(&b.id);
        self.collect_ty(&b.ty);
    }

    fn collect(&mut self, e: &Expr) {
        match e {
            Expr::Var(i) => self.var(i),
            Expr::Global(_) | Expr::Lit(_) => {}
            Expr::Panic(_, t) => self.collect_ty(t),
            Expr::Con(_, ts, a) | Expr::Prim(_, ts, a) => {
                ts.iter().for_each(|t| self.collect_ty(t));
                a.iter().for_each(|x| self.collect(x));
            }
            Expr::Extern(_, a) => a.iter().for_each(|x| self.collect(x)),
            Expr::App(f, a) => {
                self.collect(f);
                self.collect(a);
            }
            Expr::TyApp(e, t) | Expr::Cast(e, t) => {
                self.collect(e);
                self.collect_ty(t);
            }
            Expr::TyLam(v, _, e) => {
                self.tv(v);
                self.collect(e);
            }
            Expr::Lam(b, e) => {
                self.collect_binder(b);
                self.collect(e);
            }
            Expr::Let(Bind::NonRec(b, r), e) | Expr::LetStrict(b, r, e) => {
                self.collect_binder(b);
                self.collect(r);
                self.collect(e);
            }
            Expr::Let(Bind::Rec(bs), e) => {
                for (b, r) in bs {
                    self.collect_binder(b);
                    self.collect(r);
                }
                self.collect(e);
            }
            Expr::Case(s, b, t, alts) => {
                self.collect(s);
                self.collect_binder(b);
                self.collect_ty(t);
                for a in alts {
                    if let AltCon::Con(_, bs) = &a.con {
                        bs.iter().for_each(|b| self.collect_binder(b));
                    }
                    self.collect(&a.rhs);
                }
            }
        }
    }

    fn ty(&self, t: &Type) -> String {
        match t {
            Type::Var(v) => self.tvname(v),
            Type::Con(c, args) if args.is_empty() => c.clone(),
            Type::Con(c, args) => {
                let a: Vec<String> = args.iter().map(|x| self.ty(x)).collect();
                format!("({c} {})", a.join(" "))
            }
            Type::App(h, args) => {
                let a: Vec<String> = args.iter().map(|x| self.ty(x)).collect();
                format!("({} {})", self.ty(h), a.join(" "))
            }
            Type::Fun(m, _, _) => {
                // Print chains of arrows with the same multiplicity flat.
                let mut parts = Vec::new();
                let mut cur = t;
                while let Type::Fun(m2, a, b) = cur {
                    if m2 != m {
                        break;
                    }
                    parts.push(self.ty(a));
                    cur = b;
                }
                parts.push(self.ty(cur));
                match m {
                    Mult::Many => format!("(-> {})", parts.join(" ")),
                    _ => format!("(-> {} {})", m.name(), parts.join(" ")),
                }
            }
            Type::Forall(..) => {
                let mut bs = Vec::new();
                let mut cur = t;
                while let Type::Forall(v, k, b) = cur {
                    if *k == Kind::Type {
                        bs.push(self.tvname(v));
                    } else {
                        bs.push(format!("({} {k})", self.tvname(v)));
                    }
                    cur = b;
                }
                format!("(forall ({}) {})", bs.join(" "), self.ty(cur))
            }
            Type::Any => "Any".into(),
        }
    }

    fn binder(&self, b: &Binder) -> String {
        match b.mult {
            Mult::Many => format!("({} {})", self.vname(&b.id), self.ty(&b.ty)),
            m => format!("({} {} {})", self.vname(&b.id), m.name(), self.ty(&b.ty)),
        }
    }

    fn let_binding(&self, b: &Binder, rhs: &Expr, ind: usize) -> String {
        let mut s = String::new();
        let head = self.binder(b);
        s.push_str(&head[..head.len() - 1]);
        s.push(' ');
        self.expr(rhs, ind + 1, &mut s);
        s.push(')');
        s
    }

    fn nl(out: &mut String, ind: usize) {
        out.push('\n');
        for _ in 0..ind {
            out.push_str("  ");
        }
    }

    fn expr(&self, e: &Expr, ind: usize, out: &mut String) {
        match e {
            Expr::Var(i) => out.push_str(&self.vname(i)),
            Expr::Global(g) => out.push_str(g),
            Expr::Lit(Lit::Int(n)) => {
                let _ = write!(out, "{n}");
            }
            Expr::Lit(Lit::Double(x)) => {
                let s = format!("{x:?}");
                out.push_str(&s);
            }
            Expr::Lit(Lit::Str(b)) => out.push_str(&escape(b)),
            Expr::Lit(Lit::Unit) => out.push_str("unit"),
            Expr::Con(c, ts, args) => {
                let ts: Vec<String> = ts.iter().map(|t| self.ty(t)).collect();
                let _ = write!(out, "(con {c} ({})", ts.join(" "));
                for a in args {
                    out.push(' ');
                    self.expr(a, ind + 1, out);
                }
                out.push(')');
            }
            Expr::App(..) => {
                let (f, args) = e.collect_args();
                out.push('(');
                self.expr(f, ind + 1, out);
                for a in args {
                    out.push(' ');
                    self.expr(a, ind + 1, out);
                }
                out.push(')');
            }
            Expr::TyApp(..) => {
                let mut ts = Vec::new();
                let mut cur = e;
                while let Expr::TyApp(f, t) = cur {
                    ts.push(self.ty(t));
                    cur = f;
                }
                ts.reverse();
                out.push_str("(@ ");
                self.expr(cur, ind + 1, out);
                let _ = write!(out, " {})", ts.join(" "));
            }
            Expr::Lam(..) => {
                let mut bs = Vec::new();
                let mut cur = e;
                while let Expr::Lam(b, body) = cur {
                    bs.push(self.binder(b));
                    cur = body;
                }
                let _ = write!(out, "(lam ({})", bs.join(" "));
                Self::nl(out, ind + 1);
                self.expr(cur, ind + 1, out);
                out.push(')');
            }
            Expr::TyLam(..) => {
                let mut bs = Vec::new();
                let mut cur = e;
                while let Expr::TyLam(v, k, body) = cur {
                    if *k == Kind::Type {
                        bs.push(self.tvname(v));
                    } else {
                        bs.push(format!("({} {k})", self.tvname(v)));
                    }
                    cur = body;
                }
                let _ = write!(out, "(tlam ({})", bs.join(" "));
                Self::nl(out, ind + 1);
                self.expr(cur, ind + 1, out);
                out.push(')');
            }
            Expr::Let(Bind::NonRec(b, rhs), body) => {
                let _ = write!(out, "(let ({})", self.let_binding(b, rhs, ind + 1));
                Self::nl(out, ind + 1);
                self.expr(body, ind + 1, out);
                out.push(')');
            }
            Expr::Let(Bind::Rec(bs), body) => {
                out.push_str("(letrec (");
                for (i, (b, rhs)) in bs.iter().enumerate() {
                    if i > 0 {
                        Self::nl(out, ind + 4);
                    }
                    out.push_str(&self.let_binding(b, rhs, ind + 2));
                }
                out.push(')');
                Self::nl(out, ind + 1);
                self.expr(body, ind + 1, out);
                out.push(')');
            }
            Expr::LetStrict(b, rhs, body) => {
                let _ = write!(out, "(let! {}", self.let_binding(b, rhs, ind + 1));
                Self::nl(out, ind + 1);
                self.expr(body, ind + 1, out);
                out.push(')');
            }
            Expr::Case(s, b, t, alts) => {
                out.push_str("(case ");
                self.expr(s, ind + 1, out);
                let _ = write!(out, " {} {}", self.binder(b), self.ty(t));
                for a in alts {
                    Self::nl(out, ind + 1);
                    out.push('(');
                    match &a.con {
                        AltCon::Default => out.push('_'),
                        AltCon::Int(n) => {
                            let _ = write!(out, "{n}");
                        }
                        AltCon::Con(c, bs) => {
                            let _ = write!(out, "({c}");
                            for b in bs {
                                match b.mult {
                                    Mult::Many => {
                                        let _ = write!(out, " {}", self.vname(&b.id));
                                    }
                                    m => {
                                        let _ =
                                            write!(out, " ({} {})", self.vname(&b.id), m.name());
                                    }
                                }
                            }
                            out.push(')');
                        }
                    }
                    out.push(' ');
                    self.expr(&a.rhs, ind + 2, out);
                    out.push(')');
                }
                out.push(')');
            }
            Expr::Cast(e, t) => {
                out.push_str("(cast ");
                self.expr(e, ind + 1, out);
                let _ = write!(out, " {})", self.ty(t));
            }
            Expr::Prim(op, ts, args) => {
                let _ = write!(out, "(prim {}", op.name());
                if op.ty_arity() > 0 {
                    let ts: Vec<String> = ts.iter().map(|t| self.ty(t)).collect();
                    let _ = write!(out, " ({})", ts.join(" "));
                }
                for a in args {
                    out.push(' ');
                    self.expr(a, ind + 1, out);
                }
                out.push(')');
            }
            Expr::Extern(f, args) => {
                let _ = write!(out, "(extern {f}");
                for a in args {
                    out.push(' ');
                    self.expr(a, ind + 1, out);
                }
                out.push(')');
            }
            Expr::Panic(m, t) => {
                let _ = write!(out, "(panic {} {})", escape(m.as_bytes()), self.ty(t));
            }
        }
    }
}
