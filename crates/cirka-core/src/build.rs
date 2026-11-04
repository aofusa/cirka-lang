//! A small DSL for building Core in Rust (tests, frontends).
//!
//! ```
//! use cirka_core::build::B;
//! use cirka_core::{Mult, Type, Expr};
//! let mut b = B::new();
//! let x = b.binder("x", Mult::One, Type::int());
//! let id = B::lam(&x, Expr::var(&x.id));
//! assert!(matches!(id, Expr::Lam(..)));
//! ```

use crate::expr::*;
use crate::ty::*;

/// Generator of fresh variables.
pub struct B {
    next: u32,
}

impl Default for B {
    fn default() -> Self {
        B::new()
    }
}

impl B {
    pub fn new() -> B {
        B { next: 1 }
    }

    /// Continues numbering after the variables of `p`.
    pub fn after(p: &Program) -> B {
        B {
            next: p.max_id() + 1,
        }
    }

    pub fn id(&mut self, name: &str) -> Id {
        let i = Id::new(name, self.next);
        self.next += 1;
        i
    }

    pub fn tyvar(&mut self, name: &str) -> TyVar {
        let v = TyVar::new(name, self.next);
        self.next += 1;
        v
    }

    pub fn binder(&mut self, name: &str, m: Mult, ty: Type) -> Binder {
        Binder::new(self.id(name), m, ty)
    }

    pub fn lam(b: &Binder, body: Expr) -> Expr {
        Expr::Lam(b.clone(), Box::new(body))
    }

    pub fn tylam(v: &TyVar, k: Kind, body: Expr) -> Expr {
        Expr::TyLam(v.clone(), k, Box::new(body))
    }

    pub fn let_(b: &Binder, rhs: Expr, body: Expr) -> Expr {
        Expr::Let(Bind::NonRec(b.clone(), Box::new(rhs)), Box::new(body))
    }

    pub fn let_strict(b: &Binder, rhs: Expr, body: Expr) -> Expr {
        Expr::LetStrict(b.clone(), Box::new(rhs), Box::new(body))
    }

    pub fn tyapp(e: Expr, t: Type) -> Expr {
        Expr::TyApp(Box::new(e), t)
    }

    pub fn case(scrut: Expr, b: &Binder, res: Type, alts: Vec<Alt>) -> Expr {
        Expr::Case(Box::new(scrut), b.clone(), res, alts)
    }

    pub fn alt_con(c: &str, fields: Vec<Binder>, rhs: Expr) -> Alt {
        Alt {
            con: AltCon::Con(c.into(), fields),
            rhs,
        }
    }

    pub fn alt_int(n: i64, rhs: Expr) -> Alt {
        Alt {
            con: AltCon::Int(n),
            rhs,
        }
    }

    pub fn alt_default(rhs: Expr) -> Alt {
        Alt {
            con: AltCon::Default,
            rhs,
        }
    }

    pub fn prim(op: PrimOp, args: Vec<Expr>) -> Expr {
        Expr::Prim(op, vec![], args)
    }

    pub fn con(c: &str, targs: Vec<Type>, args: Vec<Expr>) -> Expr {
        Expr::Con(c.into(), targs, args)
    }
}
