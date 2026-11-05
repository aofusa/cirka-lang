//! Types of the Haskell front end (HM with higher kinds).

use std::fmt::Write;

pub const PRIM: &str = "Cirka.Prim";

pub fn prim(n: &str) -> String {
    format!("{PRIM}.{n}")
}

pub fn list_tc() -> String {
    prim("[]")
}

pub fn unit_tc() -> String {
    prim("()")
}

pub fn tuple_tc(n: usize) -> String {
    prim(&format!("({})", ",".repeat(n - 1)))
}

pub fn tuple_arity(name: &str) -> Option<usize> {
    let n = name.strip_prefix("Cirka.Prim.(")?.strip_suffix(')')?;
    if !n.is_empty() && n.chars().all(|c| c == ',') {
        Some(n.len() + 1)
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// A rigid or quantified type variable.
    Var(String),
    /// A unification variable (only during inference).
    Meta(u32),
    /// A type constructor (unapplied).
    Con(String),
    App(Box<Ty>, Box<Ty>),
    Fun(Box<Ty>, Box<Ty>),
    /// `forall vs. t`: only in the fields of class dictionaries (methods
    /// with their own type variables) and the variables bound to them.
    Forall(Vec<String>, Box<Ty>),
}

impl Ty {
    pub fn con(n: &str, args: Vec<Ty>) -> Ty {
        args.into_iter().fold(Ty::Con(n.to_string()), |f, a| {
            Ty::App(Box::new(f), Box::new(a))
        })
    }

    pub fn fun(a: Ty, b: Ty) -> Ty {
        Ty::Fun(Box::new(a), Box::new(b))
    }

    pub fn funs(args: Vec<Ty>, res: Ty) -> Ty {
        args.into_iter().rev().fold(res, |r, a| Ty::fun(a, r))
    }

    pub fn list(a: Ty) -> Ty {
        Ty::con(&list_tc(), vec![a])
    }

    /// Head and arguments of a type application.
    pub fn spine(&self) -> (&Ty, Vec<&Ty>) {
        let mut args = Vec::new();
        let mut h = self;
        while let Ty::App(f, a) = h {
            args.push(&**a);
            h = f;
        }
        args.reverse();
        (h, args)
    }

    pub fn subst(&self, s: &[(String, Ty)]) -> Ty {
        match self {
            Ty::Var(v) => s
                .iter()
                .find(|(n, _)| n == v)
                .map(|x| x.1.clone())
                .unwrap_or_else(|| self.clone()),
            Ty::Meta(_) | Ty::Con(_) => self.clone(),
            Ty::App(f, a) => Ty::App(Box::new(f.subst(s)), Box::new(a.subst(s))),
            Ty::Fun(a, b) => Ty::Fun(Box::new(a.subst(s)), Box::new(b.subst(s))),
            // Bound variables are unique names: no capture.
            Ty::Forall(vs, b) => {
                let inner: Vec<(String, Ty)> =
                    s.iter().filter(|(n, _)| !vs.contains(n)).cloned().collect();
                Ty::Forall(vs.clone(), Box::new(b.subst(&inner)))
            }
        }
    }

    pub fn vars(&self, out: &mut Vec<String>) {
        match self {
            Ty::Var(v) => {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Ty::Meta(_) | Ty::Con(_) => {}
            Ty::App(a, b) | Ty::Fun(a, b) => {
                a.vars(out);
                b.vars(out);
            }
            Ty::Forall(vs, b) => {
                let mut inner = Vec::new();
                b.vars(&mut inner);
                for v in inner {
                    if !vs.contains(&v) && !out.contains(&v) {
                        out.push(v);
                    }
                }
            }
        }
    }

    /// Head type constructor (`Maybe` of `Maybe Int`, `->` of functions).
    pub fn head_con(&self) -> Option<String> {
        match self.spine().0 {
            Ty::Con(c) => Some(c.clone()),
            Ty::Fun(..) => Some(prim("->")),
            _ => None,
        }
    }
}

/// A class constraint `C t`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pred {
    pub class: String,
    pub ty: Ty,
}

impl Pred {
    pub fn new(class: &str, ty: Ty) -> Pred {
        Pred {
            class: class.to_string(),
            ty,
        }
    }

    pub fn subst(&self, s: &[(String, Ty)]) -> Pred {
        Pred::new(&self.class, self.ty.subst(s))
    }

    /// The type of a dictionary for this constraint.
    pub fn dict_ty(&self) -> Ty {
        Ty::App(
            Box::new(Ty::Con(self.class.clone())),
            Box::new(self.ty.clone()),
        )
    }
}

/// `C t` for diagnostics.
pub fn show_pred(p: &Pred) -> String {
    let t = show(&p.ty);
    if t.contains(' ') && !t.starts_with('[') && !t.starts_with('(') {
        format!("{} ({t})", short(&p.class))
    } else {
        format!("{} {t}", short(&p.class))
    }
}

/// A type scheme `forall vs. ctx => t`. A value of the scheme takes one
/// dictionary per constraint of `preds`, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Scheme {
    pub vars: Vec<String>,
    pub preds: Vec<Pred>,
    pub ty: Ty,
}

impl Scheme {
    pub fn mono(ty: Ty) -> Scheme {
        Scheme {
            vars: Vec::new(),
            preds: Vec::new(),
            ty,
        }
    }

    pub fn poly(vars: Vec<String>, ty: Ty) -> Scheme {
        Scheme {
            vars,
            preds: Vec::new(),
            ty,
        }
    }
}

/// Kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Star,
    Arrow(Box<Kind>, Box<Kind>),
    Meta(u32),
}

impl Kind {
    pub fn arity(n: usize) -> Kind {
        (0..n).fold(Kind::Star, |k, _| {
            Kind::Arrow(Box::new(Kind::Star), Box::new(k))
        })
    }
}

/// The source name of a global or local name.
pub fn short(n: &str) -> &str {
    let n = achc_rename::display_name(n);
    // `Data.List.map` -> `map`; operators may contain dots (`Prelude..`).
    let bytes = n.as_bytes();
    let mut i = 0;
    let mut last = 0;
    while i < bytes.len() {
        // A module component starts with an upper-case letter and ends
        // with a dot.
        if bytes[i].is_ascii_uppercase() {
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'\'')
            {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'.' && j + 1 < bytes.len() {
                last = j + 1;
                i = j + 1;
                continue;
            }
        }
        break;
    }
    &n[last..]
}

/// Prints a type for diagnostics (`[a] -> Maybe Int`).
pub fn show(t: &Ty) -> String {
    let mut s = String::new();
    show_prec(t, 0, &mut s);
    s
}

fn show_prec(t: &Ty, p: u8, s: &mut String) {
    match t {
        Ty::Var(v) => s.push_str(achc_rename::display_name(v)),
        Ty::Meta(m) => {
            let _ = write!(s, "t{m}");
        }
        Ty::Con(c) => s.push_str(short(c)),
        Ty::Forall(vs, b) => {
            if p > 0 {
                s.push('(');
            }
            s.push_str("forall");
            for v in vs {
                s.push(' ');
                s.push_str(achc_rename::display_name(v));
            }
            s.push_str(". ");
            show_prec(b, 0, s);
            if p > 0 {
                s.push(')');
            }
        }
        Ty::Fun(a, b) => {
            if p > 0 {
                s.push('(');
            }
            show_prec(a, 1, s);
            s.push_str(" -> ");
            show_prec(b, 0, s);
            if p > 0 {
                s.push(')');
            }
        }
        Ty::App(..) => {
            let (h, args) = t.spine();
            if let Ty::Con(c) = h {
                if *c == list_tc() && args.len() == 1 {
                    s.push('[');
                    show_prec(args[0], 0, s);
                    s.push(']');
                    return;
                }
                if tuple_arity(c) == Some(args.len()) {
                    s.push('(');
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            s.push_str(", ");
                        }
                        show_prec(a, 0, s);
                    }
                    s.push(')');
                    return;
                }
            }
            if p > 1 {
                s.push('(');
            }
            show_prec(h, 2, s);
            for a in args {
                s.push(' ');
                show_prec(a, 2, s);
            }
            if p > 1 {
                s.push(')');
            }
        }
    }
}
