//! Fixity resolution of operator sequences (Report §10.6).

use achc_syntax::ast::{Assoc, OpItem, QName};
use cirka_diagnostics::Span;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fixity {
    pub assoc: Assoc,
    pub prec: u8,
}

pub const DEFAULT: Fixity = Fixity {
    assoc: Assoc::Left,
    prec: 9,
};

/// The operations needed to build the result.
pub trait Build<T> {
    fn fixity(&self, op: &QName) -> Fixity;
    fn binary(&mut self, op: QName, l: T, r: T) -> T;
    fn negate(&mut self, span: Span, e: T) -> Result<T, ()>;
}

pub enum Conflict {
    /// Two operators of the same precedence and different (or no)
    /// associativity.
    Ops(QName, QName),
    /// A prefix minus after an operator of higher precedence.
    Neg(Span, QName),
    Other,
}

struct Ctx<'a, T, B: Build<T>> {
    items: Vec<OpItem<T>>,
    pos: usize,
    b: &'a mut B,
}

enum Op {
    /// The virtual operator of lowest precedence at the start.
    Start,
    Neg,
    Bin(QName, Fixity),
}

impl Op {
    fn fixity(&self) -> (i32, Assoc) {
        match self {
            Op::Start => (-1, Assoc::None),
            Op::Neg => (6, Assoc::Left),
            Op::Bin(_, f) => (f.prec as i32, f.assoc),
        }
    }
}

impl<T, B: Build<T>> Ctx<'_, T, B> {
    fn take(&mut self) -> Option<OpItem<T>> {
        if self.pos >= self.items.len() {
            return None;
        }
        // Items are taken in order; replace with a dummy marker.
        let it = std::mem::replace(&mut self.items[self.pos], OpItem::Neg(Span::DUMMY));
        self.pos += 1;
        Some(it)
    }

    fn peek_op(&self) -> Option<&QName> {
        match self.items.get(self.pos) {
            Some(OpItem::Op(q)) => Some(q),
            _ => None,
        }
    }

    fn parse_neg(&mut self, op1: &Op) -> Result<T, Conflict> {
        match self.take() {
            Some(OpItem::Operand(e)) => self.parse1(op1, e),
            Some(OpItem::Neg(sp)) => {
                let (p1, _) = op1.fixity();
                if p1 >= 6 {
                    let q = match op1 {
                        Op::Bin(q, _) => q.clone(),
                        _ => return Err(Conflict::Other),
                    };
                    return Err(Conflict::Neg(sp, q));
                }
                let r = self.parse_neg(&Op::Neg)?;
                let n = self.b.negate(sp, r).map_err(|_| Conflict::Other)?;
                self.parse1(op1, n)
            }
            _ => Err(Conflict::Other),
        }
    }

    fn parse1(&mut self, op1: &Op, mut e1: T) -> Result<T, Conflict> {
        loop {
            let Some(q2) = self.peek_op().cloned() else {
                return Ok(e1);
            };
            let f2 = self.b.fixity(&q2);
            let (p1, a1) = op1.fixity();
            let (p2, a2) = (f2.prec as i32, f2.assoc);
            if p1 == p2 && (a1 != a2 || a1 == Assoc::None) {
                let q1 = match op1 {
                    Op::Bin(q, _) => q.clone(),
                    _ => QName::unqual("-", Span::DUMMY),
                };
                return Err(Conflict::Ops(q1, q2));
            }
            if p1 > p2 || (p1 == p2 && a1 == Assoc::Left) {
                return Ok(e1);
            }
            self.pos += 1;
            let op2 = Op::Bin(q2.clone(), f2);
            let r = self.parse_neg(&op2)?;
            e1 = self.b.binary(q2, e1, r);
        }
    }
}

/// Resolves `items` into a tree.
pub fn resolve<T, B: Build<T>>(items: Vec<OpItem<T>>, b: &mut B) -> Result<T, Conflict> {
    let mut c = Ctx { items, pos: 0, b };
    let r = c.parse_neg(&Op::Start)?;
    if c.pos < c.items.len() {
        return Err(Conflict::Other);
    }
    Ok(r)
}
