//! S-expression dump of the resolved program (`--dump=rn`, tests).
//! Globals are printed fully qualified; locals as `'x`.

use crate::rast::*;
use std::fmt::Write;

pub fn print_program(p: &Program) -> String {
    let mut s = String::new();
    for m in &p.modules {
        let _ = writeln!(s, "(module {}", m.name);
        for it in &m.items {
            let _ = writeln!(s, "  {}", item(p, it));
        }
        s.push_str(")\n");
    }
    if let Some(g) = p.main {
        let _ = writeln!(s, "(main {})", p.info(g).qualified());
    }
    s
}

pub fn print_module(p: &Program, name: &str) -> String {
    let mut s = String::new();
    for m in p.modules.iter().filter(|m| m.name == name) {
        for it in &m.items {
            let _ = writeln!(s, "{}", item(p, it));
        }
    }
    s
}

fn g(p: &Program, n: GName) -> String {
    p.info(n).qualified()
}

fn mult(m: &Option<Mult>) -> &'static str {
    match m {
        None => "",
        Some(Mult::Zero) => "0 ",
        Some(Mult::One) => "1 ",
        Some(Mult::Many) => "w ",
    }
}

fn item(p: &Program, it: &Item) -> String {
    match it {
        Item::Prim(n, t) => format!("(prim {} {})", g(p, *n), expr(p, t)),
        Item::Trait(t) => {
            let ms: Vec<String> = t.methods.iter().map(|m| g(p, m.name)).collect();
            format!("(trait {} ({}))", g(p, t.name), ms.join(" "))
        }
        Item::Impl(i) => {
            let ctx: Vec<String> = i.context.iter().map(|c| expr(p, c)).collect();
            let ms: Vec<String> = i.methods.iter().map(|m| g(p, m.method)).collect();
            format!(
                "(impl {} ({}) {} ({}))",
                g(p, i.name),
                ctx.join(" "),
                expr(p, &i.head),
                ms.join(" ")
            )
        }
        Item::Def(d) => {
            let mut s = format!("(def {} : {}", g(p, d.name), expr(p, &d.sig));
            for c in &d.clauses {
                let ps: Vec<String> = c.pats.iter().map(|x| pat(p, x)).collect();
                let _ = write!(s, "\n    ({}) = {}", ps.join(" "), expr(p, &c.rhs));
            }
            s.push(')');
            s
        }
        Item::Data(d) => {
            let mut s = format!("(data {}", g(p, d.name));
            for pr in &d.params {
                match &pr.ty {
                    Some(t) => {
                        let _ = write!(s, " ({}{} : {})", mult(&pr.mult), pr.name, expr(p, t));
                    }
                    None => {
                        let _ = write!(s, " {}", pr.name);
                    }
                }
            }
            if let Some(t) = &d.sig {
                let _ = write!(s, " : {}", expr(p, t));
            }
            for c in &d.cons {
                match &c.body {
                    ConBody::Sig(t) => {
                        let _ = write!(s, "\n    ({} : {})", g(p, c.name), expr(p, t));
                    }
                    ConBody::Fields(fs) => {
                        let f: Vec<String> = fs.iter().map(|x| expr(p, x)).collect();
                        let _ = write!(s, "\n    ({} {})", g(p, c.name), f.join(" "));
                    }
                }
            }
            s.push(')');
            s
        }
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

pub fn expr(p: &Program, e: &Expr) -> String {
    match &e.kind {
        ExprKind::Local(x) => format!("'{x}"),
        ExprKind::Global(n) => g(p, *n),
        ExprKind::Lit(l) => lit(l),
        ExprKind::Hole(h) => format!("?{h}"),
        ExprKind::Wildcard => "_".into(),
        ExprKind::Type => "Type".into(),
        ExprKind::LType => "LType".into(),
        ExprKind::Unit => "()".into(),
        ExprKind::App(f, a) => {
            let mut args = vec![a];
            let mut h = f;
            while let ExprKind::App(gg, b) = &h.kind {
                args.push(b);
                h = gg;
            }
            args.reverse();
            let a: Vec<String> = args.iter().map(|x| expr(p, x)).collect();
            format!("({} {})", expr(p, h), a.join(" "))
        }
        ExprKind::Lam(bs, body) => {
            let b: Vec<String> = bs
                .iter()
                .map(|b| {
                    let n = b.name.clone().unwrap_or("_".into());
                    match &b.ty {
                        Some(t) => format!("({}{n} : {})", mult(&b.mult), expr(p, t)),
                        None => n,
                    }
                })
                .collect();
            format!("(\\ ({}) {})", b.join(" "), expr(p, body))
        }
        ExprKind::Let(bs, body) => {
            let b: Vec<String> = bs.iter().map(|b| let_bind(p, b)).collect();
            format!("(let ({}) {})", b.join(" "), expr(p, body))
        }
        ExprKind::Case(s, alts) => {
            let a: Vec<String> = alts
                .iter()
                .map(|a| format!("({} => {})", pat(p, &a.pat), expr(p, &a.rhs)))
                .collect();
            format!("(case {} {})", expr(p, s), a.join(" "))
        }
        ExprKind::If(c, t, f) => format!("(if {} {} {})", expr(p, c), expr(p, t), expr(p, f)),
        ExprKind::Do(ss) => {
            let s: Vec<String> = ss
                .iter()
                .map(|s| match s {
                    Stmt::Bind(x, _, e) => format!("({x} <- {})", expr(p, e)),
                    Stmt::Let(bs) => {
                        let b: Vec<String> = bs.iter().map(|b| let_bind(p, b)).collect();
                        format!("(let {})", b.join(" "))
                    }
                    Stmt::Expr(e) => expr(p, e),
                    Stmt::Try(Some((x, _)), e, _) => format!("({x} <- {}?)", expr(p, e)),
                    Stmt::Try(None, e, _) => format!("({}?)", expr(p, e)),
                })
                .collect();
            format!("(do {})", s.join(" "))
        }
        ExprKind::Pi {
            mult: m,
            implicit,
            name,
            dom,
            cod,
            ..
        } => match name {
            None if !*implicit && m.is_none() => format!("(-> {} {})", expr(p, dom), expr(p, cod)),
            _ => {
                let n = name.clone().unwrap_or("_".into());
                if *implicit {
                    format!(
                        "(-> {{{}{n} : {}}} {})",
                        mult(m),
                        expr(p, dom),
                        expr(p, cod)
                    )
                } else {
                    format!("(-> ({}{n} : {}) {})", mult(m), expr(p, dom), expr(p, cod))
                }
            }
        },
        ExprKind::Ann(e, t) => format!("(the {} {})", expr(p, t), expr(p, e)),
        ExprKind::Tuple(es) => {
            let a: Vec<String> = es.iter().map(|x| expr(p, x)).collect();
            format!("(tuple {})", a.join(" "))
        }
        ExprKind::List(es) => {
            let a: Vec<String> = es.iter().map(|x| expr(p, x)).collect();
            format!("[{}]", a.join(" "))
        }
    }
}

fn let_bind(p: &Program, b: &LetBind) -> String {
    let mut s = format!("({}{}", mult(&b.mult), b.name);
    if let Some(t) = &b.ty {
        let _ = write!(s, " : {}", expr(p, t));
    }
    let _ = write!(s, " = {})", expr(p, &b.rhs));
    s
}

pub fn pat(p: &Program, x: &Pat) -> String {
    match &x.kind {
        PatKind::Var(v) => v.clone(),
        PatKind::Wild => "_".into(),
        PatKind::Con(c, args) if args.is_empty() => g(p, *c),
        PatKind::Con(c, args) => {
            let a: Vec<String> = args.iter().map(|y| pat(p, y)).collect();
            format!("({} {})", g(p, *c), a.join(" "))
        }
        PatKind::Lit(l, neg) => format!("{}{}", if *neg { "-" } else { "" }, lit(l)),
        PatKind::Unit => "()".into(),
        PatKind::Tuple(ps) => {
            let a: Vec<String> = ps.iter().map(|y| pat(p, y)).collect();
            format!("(tuple {})", a.join(" "))
        }
        PatKind::List(ps) => {
            let a: Vec<String> = ps.iter().map(|y| pat(p, y)).collect();
            format!("[{}]", a.join(" "))
        }
    }
}
