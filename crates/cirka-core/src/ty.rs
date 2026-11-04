//! Kinds, multiplicities and types of the common Core (06-ir.md §1).

use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

/// Multiplicity of a binder or function argument (0, 1, ω).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mult {
    Zero,
    One,
    Many,
}

impl Mult {
    /// Semiring addition: 1 + 1 = ω.
    pub fn plus(self, o: Mult) -> Mult {
        match (self, o) {
            (Mult::Zero, m) | (m, Mult::Zero) => m,
            _ => Mult::Many,
        }
    }

    /// Semiring multiplication.
    pub fn times(self, o: Mult) -> Mult {
        match (self, o) {
            (Mult::Zero, _) | (_, Mult::Zero) => Mult::Zero,
            (Mult::One, m) | (m, Mult::One) => m,
            _ => Mult::Many,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mult::Zero => "0",
            Mult::One => "1",
            Mult::Many => "ω",
        }
    }
}

/// Runtime representation of an unboxed type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rep {
    I64,
    F64,
}

/// Kinds. `Type ⊑ LType` (03-type-system.md §1).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Boxed, shareable values.
    Type,
    /// Boxed, linear-only values.
    LType,
    /// Unboxed scalars (`Int#`, `Double#`, `Unit#`): freely copyable.
    Unboxed,
    Arrow(Box<Kind>, Box<Kind>),
}

impl Kind {
    /// `self ⊑ other`.
    pub fn sub(&self, other: &Kind) -> bool {
        matches!((self, other), (Kind::Type, Kind::LType)) || self == other
    }

    pub fn lub(&self, other: &Kind) -> Kind {
        match (self, other) {
            (Kind::LType, _) | (_, Kind::LType) => Kind::LType,
            (a, _) => a.clone(),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Type => write!(f, "Type"),
            Kind::LType => write!(f, "LType"),
            Kind::Unboxed => write!(f, "#"),
            Kind::Arrow(a, b) => write!(f, "(-> {a} {b})"),
        }
    }
}

/// A type variable: printed name plus a unique number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TyVar {
    pub name: String,
    pub id: u32,
}

static NEXT_TYVAR: AtomicU32 = AtomicU32::new(1 << 24);

impl TyVar {
    pub fn new(name: impl Into<String>, id: u32) -> TyVar {
        TyVar {
            name: name.into(),
            id,
        }
    }

    /// A globally fresh type variable (used for capture-avoiding renaming).
    pub fn fresh(name: &str) -> TyVar {
        TyVar {
            name: name.to_string(),
            id: NEXT_TYVAR.fetch_add(1, Ordering::Relaxed),
        }
    }
}

/// Built-in unboxed type constructors.
pub const INT: &str = "Int#";
pub const DOUBLE: &str = "Double#";
pub const UNIT: &str = "Unit#";
/// Built-in boxed type constructors.
pub const BYTES: &str = "Bytes";
pub const ARRAY: &str = "Array";

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Var(TyVar),
    /// Type constructor applied to arguments (`List a`, `Int#`). May be
    /// partially applied when passed to a higher-kinded variable.
    Con(String, Vec<Type>),
    /// Type variable applied to arguments (`f a` with `f : Type -> Type`).
    App(Box<Type>, Vec<Type>),
    /// `a ->π b`.
    Fun(Mult, Box<Type>, Box<Type>),
    Forall(TyVar, Kind, Box<Type>),
    /// Any boxed value (result of erasing dependent types).
    Any,
}

impl Type {
    pub fn con(name: &str) -> Type {
        Type::Con(name.to_string(), vec![])
    }

    pub fn int() -> Type {
        Type::con(INT)
    }

    pub fn double() -> Type {
        Type::con(DOUBLE)
    }

    pub fn unit() -> Type {
        Type::con(UNIT)
    }

    pub fn bytes() -> Type {
        Type::con(BYTES)
    }

    /// `head args`, normalized (`(Maybe) a` becomes `Maybe a`).
    pub fn mk_app(head: Type, args: Vec<Type>) -> Type {
        if args.is_empty() {
            return head;
        }
        match head {
            Type::Con(c, mut xs) => {
                xs.extend(args);
                Type::Con(c, xs)
            }
            Type::App(h, mut xs) => {
                xs.extend(args);
                Type::App(h, xs)
            }
            h => Type::App(Box::new(h), args),
        }
    }

    pub fn fun(m: Mult, a: Type, b: Type) -> Type {
        Type::Fun(m, Box::new(a), Box::new(b))
    }

    /// Unboxed representation, or `None` for boxed types.
    pub fn rep(&self) -> Option<Rep> {
        match self {
            Type::Con(c, _) if c == INT || c == UNIT => Some(Rep::I64),
            Type::Con(c, _) if c == DOUBLE => Some(Rep::F64),
            _ => None,
        }
    }

    pub fn is_unboxed(&self) -> bool {
        self.rep().is_some()
    }

    /// Free type variables.
    pub fn free_vars(&self, out: &mut Vec<TyVar>) {
        match self {
            Type::Var(v) => {
                if !out.contains(v) {
                    out.push(v.clone())
                }
            }
            Type::Con(_, args) => args.iter().for_each(|a| a.free_vars(out)),
            Type::App(h, args) => {
                h.free_vars(out);
                args.iter().for_each(|a| a.free_vars(out));
            }
            Type::Fun(_, a, b) => {
                a.free_vars(out);
                b.free_vars(out);
            }
            Type::Forall(v, _, b) => {
                let mut inner = Vec::new();
                b.free_vars(&mut inner);
                for x in inner {
                    if &x != v && !out.contains(&x) {
                        out.push(x);
                    }
                }
            }
            Type::Any => {}
        }
    }

    pub fn mentions(&self, v: &TyVar) -> bool {
        let mut fv = Vec::new();
        self.free_vars(&mut fv);
        fv.contains(v)
    }

    /// Capture-avoiding substitution `self[t/v]`.
    pub fn subst(&self, v: &TyVar, t: &Type) -> Type {
        match self {
            Type::Var(x) if x == v => t.clone(),
            Type::Var(_) | Type::Any => self.clone(),
            Type::Con(c, args) => {
                Type::Con(c.clone(), args.iter().map(|a| a.subst(v, t)).collect())
            }
            Type::App(h, args) => {
                Type::mk_app(h.subst(v, t), args.iter().map(|a| a.subst(v, t)).collect())
            }
            Type::Fun(m, a, b) => Type::fun(*m, a.subst(v, t), b.subst(v, t)),
            Type::Forall(x, k, b) => {
                if x == v {
                    return self.clone();
                }
                if t.mentions(x) {
                    let y = TyVar::fresh(&x.name);
                    let b2 = b.subst(x, &Type::Var(y.clone()));
                    Type::Forall(y, k.clone(), Box::new(b2.subst(v, t)))
                } else {
                    Type::Forall(x.clone(), k.clone(), Box::new(b.subst(v, t)))
                }
            }
        }
    }

    /// Simultaneous substitution of several variables.
    pub fn subst_all(&self, s: &[(TyVar, Type)]) -> Type {
        // Rename to fresh variables first so the substitutions do not interfere.
        let mut t = self.clone();
        let fresh: Vec<TyVar> = s.iter().map(|(v, _)| TyVar::fresh(&v.name)).collect();
        for ((v, _), f) in s.iter().zip(&fresh) {
            t = t.subst(v, &Type::Var(f.clone()));
        }
        for ((_, ty), f) in s.iter().zip(&fresh) {
            t = t.subst(f, ty);
        }
        t
    }

    /// Alpha-equivalence.
    pub fn alpha_eq(&self, other: &Type) -> bool {
        fn go(a: &Type, b: &Type, env: &mut Vec<(TyVar, TyVar)>) -> bool {
            match (a, b) {
                (Type::Var(x), Type::Var(y)) => {
                    for (l, r) in env.iter().rev() {
                        if l == x || r == y {
                            return l == x && r == y;
                        }
                    }
                    x == y
                }
                (Type::Con(c, xs), Type::Con(d, ys)) => {
                    c == d && xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| go(x, y, env))
                }
                (Type::App(h1, xs), Type::App(h2, ys)) => {
                    go(h1, h2, env)
                        && xs.len() == ys.len()
                        && xs.iter().zip(ys).all(|(x, y)| go(x, y, env))
                }
                (Type::Fun(m, a1, b1), Type::Fun(n, a2, b2)) => {
                    m == n && go(a1, a2, env) && go(b1, b2, env)
                }
                (Type::Forall(x, k, b1), Type::Forall(y, l, b2)) => {
                    if k != l {
                        return false;
                    }
                    env.push((x.clone(), y.clone()));
                    let r = go(b1, b2, env);
                    env.pop();
                    r
                }
                (Type::Any, Type::Any) => true,
                _ => false,
            }
        }
        go(self, other, &mut Vec::new())
    }

    /// Splits `∀ā. τ₁ →π₁ … → τₙ →πₙ ρ` into its parts (no type args).
    pub fn split_fun(&self) -> (Vec<(Mult, Type)>, Type) {
        let mut args = Vec::new();
        let mut t = self;
        while let Type::Fun(m, a, b) = t {
            args.push((*m, (**a).clone()));
            t = b;
        }
        (args, t.clone())
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(v) => write!(f, "{}", v.name),
            Type::Con(c, args) if args.is_empty() => write!(f, "{c}"),
            Type::Con(c, args) => {
                write!(f, "({c}")?;
                for a in args {
                    write!(f, " {a}")?;
                }
                write!(f, ")")
            }
            Type::App(h, args) => {
                write!(f, "({h}")?;
                for a in args {
                    write!(f, " {a}")?;
                }
                write!(f, ")")
            }
            Type::Fun(Mult::Many, a, b) => write!(f, "(-> {a} {b})"),
            Type::Fun(m, a, b) => write!(f, "(-> {} {a} {b})", m.name()),
            Type::Forall(v, k, b) => write!(f, "(forall ({} {k}) {b})", v.name),
            Type::Any => write!(f, "Any"),
        }
    }
}
