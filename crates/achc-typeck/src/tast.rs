//! The typed program produced by the type checker: surface sugar is gone
//! except pattern matching, which the desugarer compiles.

use crate::types::*;
use cirka_diagnostics::Span;
use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    Double(f64),
    Char(char),
    /// A string literal of type `String` (`[Char]`).
    Str(String),
}

#[derive(Clone, Debug)]
pub enum Expr {
    Local(String, Ty),
    /// A global with the instantiation of its scheme's variables.
    Global(String, Vec<Ty>),
    /// A constructor with the instantiation of its data type's parameters.
    Con(String, Vec<Ty>),
    Lit(Lit),
    App(Box<Expr>, Box<Expr>),
    Lam(String, Ty, Box<Expr>),
    Let(Vec<BindGroup>, Box<Expr>),
    /// Matches the variables against the clauses in order; falls through
    /// on pattern or guard failure.
    Match(Vec<(String, Ty)>, Vec<Clause>, Ty),
    /// Runtime error (missing record field, failed match).
    Error(String, Ty),
    /// A class dictionary to be found by constraint solving (only during
    /// type checking).
    Hole(u32),
    /// An overloaded numeric literal at a type with the `Num` /
    /// `Fractional` dictionary in a hole (only during type checking).
    OverLit(Lit, Ty, u32),
}

#[derive(Clone, Debug)]
pub struct Clause {
    pub pats: Vec<Pat>,
    pub rhs: Rhs,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Rhs {
    pub wheres: Vec<BindGroup>,
    pub body: Vec<(Vec<Guard>, Expr)>,
}

#[derive(Clone, Debug)]
pub enum Guard {
    Bool(Expr),
    Bind(Pat, Expr),
    Let(Vec<BindGroup>),
}

#[derive(Clone, Debug)]
pub enum Pat {
    Var(String, Ty),
    Wild(Ty),
    Lit(Lit, Ty),
    Con(String, Vec<Ty>, Vec<Pat>),
    As(String, Ty, Box<Pat>),
    Lazy(Box<Pat>),
    Bang(Box<Pat>),
    /// An overloaded numeric literal: matches when the `Bool`-valued test
    /// (`(== lit)`) applied to the scrutinee is `True`.
    OverLit(Lit, Box<Expr>, Ty),
}

#[derive(Clone, Debug)]
pub enum BindGroup {
    NonRec(Bind),
    Rec(Vec<Bind>),
}

impl BindGroup {
    pub fn binds(&self) -> &[Bind] {
        match self {
            BindGroup::NonRec(b) => std::slice::from_ref(b),
            BindGroup::Rec(bs) => bs,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bind {
    pub name: String,
    /// Quantified type variables with their kinds.
    pub tvs: Vec<(String, Kind)>,
    pub ty: Ty,
    pub rhs: Expr,
    pub span: Span,
    /// Evaluated before the body of the `let` (a bang pattern binding).
    pub strict: bool,
}

#[derive(Clone, Debug)]
pub struct DataDecl {
    pub name: String,
    pub params: Vec<(String, Kind)>,
    pub cons: Vec<ConDecl>,
    pub newtype: bool,
}

#[derive(Clone, Debug)]
pub struct ConDecl {
    pub name: String,
    pub fields: Vec<Ty>,
    pub strict: Vec<bool>,
}

/// `foreign import prim "name"` or `foreign import ccall "name"`.
#[derive(Clone, Debug)]
pub struct Foreign {
    pub name: String,
    pub callconv: String,
    pub entity: String,
    pub tvs: Vec<(String, Kind)>,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub datas: Vec<DataDecl>,
    pub foreigns: Vec<Foreign>,
    /// Top-level bindings of all modules in dependency order.
    pub binds: Vec<BindGroup>,
    /// `Main.main`, if present.
    pub main: Option<String>,
    pub warnings: Vec<cirka_diagnostics::Diagnostic>,
}

// ---- printing (`--dump=tc`) ----

pub fn print_program(p: &Program) -> String {
    let mut s = String::new();
    for d in &p.datas {
        let ps: Vec<&str> = d.params.iter().map(|x| x.0.as_str()).collect();
        let _ = writeln!(s, "data {} {}", d.name, ps.join(" "));
        for c in &d.cons {
            let fs: Vec<String> = c
                .fields
                .iter()
                .zip(&c.strict)
                .map(|(t, st)| format!("{}{}", if *st { "!" } else { "" }, show_atom(t)))
                .collect();
            let _ = writeln!(s, "  {} {}", c.name, fs.join(" "));
        }
    }
    for f in &p.foreigns {
        let _ = writeln!(
            s,
            "foreign {} {:?} {} :: {}",
            f.callconv,
            f.entity,
            f.name,
            show(&f.ty)
        );
    }
    for g in &p.binds {
        print_group(g, 0, &mut s);
    }
    s
}

/// The bindings of one module (snapshot tests).
pub fn print_module_binds(p: &Program, module: &str) -> String {
    let prefix = format!("{module}.");
    let mut s = String::new();
    for g in &p.binds {
        if g.binds().iter().any(|b| b.name.starts_with(&prefix)) {
            print_group(g, 0, &mut s);
        }
    }
    s
}

fn show_atom(t: &Ty) -> String {
    let x = show(t);
    if x.contains(' ') && !x.starts_with('[') && !x.starts_with('(') {
        format!("({x})")
    } else {
        x
    }
}

fn indent(n: usize, s: &mut String) {
    for _ in 0..n {
        s.push_str("  ");
    }
}

fn print_group(g: &BindGroup, n: usize, s: &mut String) {
    if let BindGroup::Rec(_) = g {
        indent(n, s);
        s.push_str("rec\n");
    }
    for b in g.binds() {
        indent(n, s);
        let tvs: Vec<&str> = b
            .tvs
            .iter()
            .map(|x| achc_rename::display_name(&x.0))
            .collect();
        let q = if tvs.is_empty() {
            String::new()
        } else {
            format!("forall {}. ", tvs.join(" "))
        };
        let _ = writeln!(s, "{} :: {q}{}", b.name, show(&b.ty));
        indent(n, s);
        let _ = write!(s, "{} = ", b.name);
        expr(&b.rhs, n + 1, s);
        s.push('\n');
    }
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Double(x) => format!("{x:?}"),
        Lit::Char(c) => format!("{c:?}"),
        Lit::Str(t) => format!("{t:?}"),
    }
}

fn tys(ts: &[Ty]) -> String {
    ts.iter().map(|t| format!(" @{}", show_atom(t))).collect()
}

pub fn pat(p: &Pat) -> String {
    match p {
        Pat::Var(v, _) => v.clone(),
        Pat::Wild(_) => "_".into(),
        Pat::Lit(l, _) => lit(l),
        Pat::Con(c, _, ps) if ps.is_empty() => short(c).to_string(),
        Pat::Con(c, _, ps) => {
            let v: Vec<String> = ps.iter().map(pat).collect();
            format!("({} {})", short(c), v.join(" "))
        }
        Pat::As(v, _, p) => format!("{v}@{}", pat(p)),
        Pat::Lazy(p) => format!("~{}", pat(p)),
        Pat::Bang(p) => format!("!{}", pat(p)),
        Pat::OverLit(l, _, _) => format!("{}?", lit(l)),
    }
}

fn expr(e: &Expr, n: usize, s: &mut String) {
    match e {
        Expr::Local(v, _) => s.push_str(v),
        Expr::Global(g, ts) | Expr::Con(g, ts) => {
            s.push_str(g);
            s.push_str(&tys(ts));
        }
        Expr::Lit(l) => s.push_str(&lit(l)),
        Expr::App(f, a) => {
            s.push('(');
            expr(f, n, s);
            s.push(' ');
            expr(a, n, s);
            s.push(')');
        }
        Expr::Lam(x, t, b) => {
            let _ = write!(s, "(\\({x} :: {}) -> ", show(t));
            expr(b, n, s);
            s.push(')');
        }
        Expr::Let(gs, b) => {
            s.push_str("let\n");
            for g in gs {
                print_group(g, n + 1, s);
            }
            indent(n, s);
            s.push_str("in ");
            expr(b, n, s);
        }
        Expr::Match(vs, cls, _) => {
            let v: Vec<&str> = vs.iter().map(|x| x.0.as_str()).collect();
            let _ = write!(s, "match {} {{", v.join(" "));
            for c in cls {
                s.push('\n');
                indent(n + 1, s);
                let ps: Vec<String> = c.pats.iter().map(pat).collect();
                s.push_str(&ps.join(" "));
                rhs(&c.rhs, n + 2, s);
            }
            s.push('\n');
            indent(n, s);
            s.push('}');
        }
        Expr::Error(m, _) => {
            let _ = write!(s, "error {m:?}");
        }
        Expr::Hole(h) => {
            let _ = write!(s, "?dict{h}");
        }
        Expr::OverLit(l, _, h) => {
            let _ = write!(s, "(fromLit ?dict{h} {})", lit(l));
        }
    }
}

fn rhs(r: &Rhs, n: usize, s: &mut String) {
    for (gs, e) in &r.body {
        if gs.is_empty() {
            s.push_str(" -> ");
        } else {
            s.push_str(" | ");
            for (i, g) in gs.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                match g {
                    Guard::Bool(e) => expr(e, n, s),
                    Guard::Bind(p, e) => {
                        let _ = write!(s, "{} <- ", pat(p));
                        expr(e, n, s);
                    }
                    Guard::Let(_) => s.push_str("let .."),
                }
            }
            s.push_str(" -> ");
        }
        expr(e, n, s);
    }
    if !r.wheres.is_empty() {
        s.push('\n');
        indent(n, s);
        s.push_str("where\n");
        for g in &r.wheres {
            print_group(g, n + 1, s);
        }
    }
}
