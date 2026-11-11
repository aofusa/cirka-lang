//! S-expression dump of the AST (`--dump=ast`, snapshot tests).

use crate::ast::*;
use std::fmt::Write;

pub fn print_module(m: &Module) -> String {
    let mut s = String::new();
    if let Some((n, _)) = &m.name {
        let _ = writeln!(s, "(module {})", n.join("."));
    }
    for i in &m.imports {
        let _ = writeln!(s, "(import {})", i.path.join("."));
    }
    for d in &m.decls {
        s.push_str(&decl(d));
        s.push('\n');
    }
    s
}

fn mult(m: &Option<Mult>) -> &'static str {
    match m {
        None => "",
        Some(Mult::Zero) => "0 ",
        Some(Mult::One) => "1 ",
        Some(Mult::Many) => "w ",
    }
}

fn names(ns: &[(String, cirka_diagnostics::Span)]) -> String {
    ns.iter()
        .map(|n| n.0.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn decl(d: &Decl) -> String {
    let p = if d.private { "private " } else { "" };
    match &d.kind {
        DeclKind::Fixity { assoc, prec, ops } => {
            let a = match assoc {
                Assoc::Left => "infixl",
                Assoc::Right => "infixr",
                Assoc::None => "infix",
            };
            format!("({p}{a} {prec} {})", names(ops))
        }
        DeclKind::Sig { names: ns, ty } => format!("({p}sig ({}) {})", names(ns), expr(ty)),
        DeclKind::Clause {
            name, pats, rhs, ..
        } => {
            let ps: Vec<String> = pats.iter().map(pat).collect();
            format!("({p}def {name} ({}) {})", ps.join(" "), expr(rhs))
        }
        DeclKind::Prim { name, ty, .. } => format!("({p}prim {name} {})", expr(ty)),
        DeclKind::Test { name, body, .. } => format!("({p}test {name:?} {})", expr(body)),
        DeclKind::Tests(ds) => {
            let ds: Vec<String> = ds.iter().map(decl).collect();
            format!("({p}tests {})", ds.join(" "))
        }
        DeclKind::Trait(t) => {
            let ctx: Vec<String> = t.context.iter().map(expr).collect();
            let ps: Vec<String> = t.params.iter().map(param).collect();
            let items: Vec<String> = t.items.iter().map(decl).collect();
            format!(
                "({p}trait ({}) {} ({}) {})",
                ctx.join(" "),
                t.name,
                ps.join(" "),
                items.join(" ")
            )
        }
        DeclKind::Impl(i) => {
            let ctx: Vec<String> = i.context.iter().map(expr).collect();
            let items: Vec<String> = i.items.iter().map(decl).collect();
            format!(
                "({p}impl ({}) {} {})",
                ctx.join(" "),
                expr(&i.head),
                items.join(" ")
            )
        }
        DeclKind::Data(dd) => {
            let mut s = format!("({p}data {}", dd.name);
            for pr in &dd.params {
                let _ = write!(s, " {}", param(pr));
            }
            if let Some(t) = &dd.sig {
                let _ = write!(s, " : {}", expr(t));
            }
            for c in &dd.cons {
                match &c.body {
                    ConBody::Sig(t) => {
                        let _ = write!(s, "\n  ({} : {})", c.name, expr(t));
                    }
                    ConBody::Fields(fs) => {
                        let f: Vec<String> = fs.iter().map(expr).collect();
                        let _ = write!(s, "\n  ({} {})", c.name, f.join(" "));
                    }
                }
            }
            if !dd.deriving.is_empty() {
                let ds: Vec<String> = dd.deriving.iter().map(|d| d.0.to_string()).collect();
                let _ = write!(s, "\n  (deriving {})", ds.join(" "));
            }
            s.push(')');
            s
        }
    }
}

fn param(pr: &Param) -> String {
    match &pr.ty {
        Some(t) => format!("({}{} : {})", mult(&pr.mult), names(&pr.names), expr(t)),
        None => names(&pr.names),
    }
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Float(x) => format!("{x:?}"),
        Lit::Char(c) => format!("{c:?}"),
        Lit::Str(s) => format!("{s:?}"),
    }
}

fn op(o: &OpRef) -> String {
    if o.backtick {
        format!("`{}`", o.name)
    } else {
        o.name.to_string()
    }
}

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Var(q) => q.to_string(),
        ExprKind::Lit(l) => lit(l),
        ExprKind::Hole(h) => format!("?{h}"),
        ExprKind::Wildcard => "_".into(),
        ExprKind::Type => "Type".into(),
        ExprKind::LType => "LType".into(),
        ExprKind::Unit => "()".into(),
        ExprKind::App(f, a) => {
            let mut args = vec![a];
            let mut h = f;
            while let ExprKind::App(g, b) = &h.kind {
                args.push(b);
                h = g;
            }
            args.reverse();
            let a: Vec<String> = args.iter().map(|x| expr(x)).collect();
            format!("({} {})", expr(h), a.join(" "))
        }
        ExprKind::Lam(bs, body) => {
            let b: Vec<String> = bs
                .iter()
                .map(|b| {
                    let n = b.name.clone().unwrap_or("_".into());
                    match &b.ty {
                        Some(t) => format!("({}{n} : {})", mult(&b.mult), expr(t)),
                        None => n,
                    }
                })
                .collect();
            format!("(\\ ({}) {})", b.join(" "), expr(body))
        }
        ExprKind::Let(bs, body) => {
            let b: Vec<String> = bs.iter().map(let_bind).collect();
            format!("(let ({}) {})", b.join(" "), expr(body))
        }
        ExprKind::Case(s, alts) => {
            let a: Vec<String> = alts
                .iter()
                .map(|a| format!("({} => {})", pat(&a.pat), expr(&a.rhs)))
                .collect();
            format!("(case {} {})", expr(s), a.join(" "))
        }
        ExprKind::If(c, t, f) => format!("(if {} {} {})", expr(c), expr(t), expr(f)),
        ExprKind::Do(ss) => {
            let s: Vec<String> = ss
                .iter()
                .map(|s| match s {
                    Stmt::Bind(x, _, e) => format!("({x} <- {})", expr(e)),
                    Stmt::Let(bs) => {
                        let b: Vec<String> = bs.iter().map(let_bind).collect();
                        format!("(let {})", b.join(" "))
                    }
                    Stmt::Expr(e) => expr(e),
                })
                .collect();
            format!("(do {})", s.join(" "))
        }
        ExprKind::Pi {
            mult: m,
            implicit,
            names: ns,
            dom,
            cod,
        } => {
            if ns.is_empty() {
                format!("(-> {} {})", expr(dom), expr(cod))
            } else if *implicit {
                format!(
                    "(-> {{{}{} : {}}} {})",
                    mult(m),
                    names(ns),
                    expr(dom),
                    expr(cod)
                )
            } else {
                format!(
                    "(-> ({}{} : {}) {})",
                    mult(m),
                    names(ns),
                    expr(dom),
                    expr(cod)
                )
            }
        }
        ExprKind::Ann(e, t) => format!("(the {} {})", expr(t), expr(e)),
        ExprKind::Tuple(es) => {
            let a: Vec<String> = es.iter().map(expr).collect();
            format!("(tuple {})", a.join(" "))
        }
        ExprKind::List(es) => {
            let a: Vec<String> = es.iter().map(expr).collect();
            format!("[{}]", a.join(" "))
        }
        ExprKind::OpSeq { neg, operands, ops } => {
            let mut s = String::from("(ops");
            if neg.is_some() {
                s.push_str(" neg");
            }
            for (i, x) in operands.iter().enumerate() {
                if i > 0 {
                    let _ = write!(s, " {}", op(&ops[i - 1]));
                }
                let _ = write!(s, " {}", expr(x));
            }
            s.push(')');
            s
        }
        ExprKind::LeftSection(e, o) => format!("(section {} {})", expr(e), op(o)),
        ExprKind::RightSection(o, e) => format!("(section {} {})", op(o), expr(e)),
    }
}

fn let_bind(b: &LetBind) -> String {
    let mut s = format!("({}{}", mult(&b.mult), b.name);
    for a in &b.args {
        let _ = write!(s, " {}", pat(a));
    }
    if let Some(t) = &b.ty {
        let _ = write!(s, " : {}", expr(t));
    }
    let _ = write!(s, " = {})", expr(&b.rhs));
    s
}

pub fn pat(p: &Pat) -> String {
    match &p.kind {
        PatKind::Var(x) => x.clone(),
        PatKind::Wild => "_".into(),
        PatKind::Con(c, args) if args.is_empty() => c.to_string(),
        PatKind::Con(c, args) => {
            let a: Vec<String> = args.iter().map(pat).collect();
            format!("({c} {})", a.join(" "))
        }
        PatKind::Lit(l, neg) => format!("{}{}", if *neg { "-" } else { "" }, lit(l)),
        PatKind::Unit => "()".into(),
        PatKind::Tuple(ps) => {
            let a: Vec<String> = ps.iter().map(pat).collect();
            format!("(tuple {})", a.join(" "))
        }
        PatKind::List(ps) => {
            let a: Vec<String> = ps.iter().map(pat).collect();
            format!("[{}]", a.join(" "))
        }
        PatKind::OpSeq(ps, ops) => {
            let mut s = String::from("(ops");
            for (i, x) in ps.iter().enumerate() {
                if i > 0 {
                    let _ = write!(s, " {}", op(&ops[i - 1]));
                }
                let _ = write!(s, " {}", pat(x));
            }
            s.push(')');
            s
        }
    }
}
