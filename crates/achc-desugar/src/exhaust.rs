//! Exhaustiveness of equations by the usefulness algorithm (Maranget,
//! "Warnings for pattern matching", 2007): the match is non-exhaustive
//! when the all-wildcard vector is useful with respect to the equations
//! that cannot fall through. Guards other than `otherwise` / `True` may
//! fail, so equations with them do not count.

use crate::{Ds, is_true};
use achc_typeck::tast::{self, Clause, Guard, Pat};

/// A pattern reduced to what matters for coverage.
#[derive(Clone, Debug)]
enum Sp {
    Wild,
    /// A constructor (by name) with its argument patterns.
    Con(String, Vec<Sp>),
    /// A literal (never part of a complete signature).
    Lit(String),
}

impl Ds {
    /// Whether some value is matched by no equation of `cls`.
    pub(crate) fn non_exhaustive(&mut self, cls: &[Clause]) -> bool {
        let Some(n) = cls.first().map(|c| c.pats.len()) else {
            return true;
        };
        let rows: Vec<Vec<Sp>> = cls
            .iter()
            .filter(|c| total_rhs(&c.rhs))
            .map(|c| c.pats.iter().map(|p| self.simplify(p)).collect())
            .collect();
        self.useful(&rows, &vec![Sp::Wild; n])
    }

    fn simplify(&mut self, p: &Pat) -> Sp {
        match p {
            Pat::Var(..) | Pat::Wild(_) | Pat::Lazy(_) => Sp::Wild,
            Pat::As(_, _, q) | Pat::Bang(q) => self.simplify(q),
            Pat::Con(c, _, ps) => Sp::Con(c.clone(), ps.iter().map(|q| self.simplify(q)).collect()),
            Pat::Lit(tast::Lit::Str(s), _) => {
                // A string literal is a list of characters.
                let nil = Sp::Con(achc_typeck::types::list_tc(), Vec::new());
                s.chars().rev().fold(nil, |acc, ch| {
                    Sp::Con(
                        achc_typeck::types::prim(":"),
                        vec![Sp::Lit(format!("{ch:?}")), acc],
                    )
                })
            }
            Pat::Lit(l, _) | Pat::OverLit(l, _, _) => Sp::Lit(format!("{l:?}")),
        }
    }

    fn arity(&mut self, c: &str) -> usize {
        self.con(c).fields.len()
    }

    /// All constructors of the data type of constructor `c`.
    fn siblings(&mut self, c: &str) -> Vec<String> {
        let data = self.con(c).data;
        self.datas
            .get(&data)
            .map(|d| d.cons.clone())
            .unwrap_or_default()
    }

    fn useful(&mut self, rows: &[Vec<Sp>], q: &[Sp]) -> bool {
        let Some((head, rest)) = q.split_first() else {
            return rows.is_empty();
        };
        match head {
            Sp::Con(c, args) => {
                let spec = self.specialize(rows, &Head::Con(c.clone()));
                let mut q2 = args.clone();
                q2.extend(rest.iter().cloned());
                self.useful(&spec, &q2)
            }
            Sp::Lit(l) => {
                let spec = self.specialize(rows, &Head::Lit(l.clone()));
                self.useful(&spec, rest)
            }
            Sp::Wild => {
                let mut seen: Vec<String> = Vec::new();
                for r in rows {
                    if let Sp::Con(c, _) = &r[0]
                        && !seen.contains(c)
                    {
                        seen.push(c.clone());
                    }
                }
                let complete = match seen.first() {
                    Some(c) => {
                        let all = self.siblings(c);
                        !all.is_empty() && all.iter().all(|k| seen.contains(k))
                    }
                    None => false,
                };
                if complete {
                    for c in seen {
                        let k = self.arity(&c);
                        let spec = self.specialize(rows, &Head::Con(c));
                        let mut q2 = vec![Sp::Wild; k];
                        q2.extend(rest.iter().cloned());
                        if self.useful(&spec, &q2) {
                            return true;
                        }
                    }
                    false
                } else {
                    let def: Vec<Vec<Sp>> = rows
                        .iter()
                        .filter(|r| matches!(r[0], Sp::Wild))
                        .map(|r| r[1..].to_vec())
                        .collect();
                    self.useful(&def, rest)
                }
            }
        }
    }

    /// The rows whose first pattern matches `h`, with that column
    /// replaced by the constructor's arguments.
    fn specialize(&mut self, rows: &[Vec<Sp>], h: &Head) -> Vec<Vec<Sp>> {
        let k = match h {
            Head::Con(c) => self.arity(c),
            Head::Lit(_) => 0,
        };
        let mut out = Vec::new();
        for r in rows {
            let mut nr = match (&r[0], h) {
                (Sp::Wild, _) => vec![Sp::Wild; k],
                (Sp::Con(c, args), Head::Con(d)) if c == d => args.clone(),
                (Sp::Lit(l), Head::Lit(m)) if l == m => Vec::new(),
                _ => continue,
            };
            nr.extend(r[1..].iter().cloned());
            out.push(nr);
        }
        out
    }
}

enum Head {
    Con(String),
    Lit(String),
}

/// Whether the right-hand side always produces a value (no guard can fail).
fn total_rhs(r: &tast::Rhs) -> bool {
    r.body.iter().any(|(gs, _)| {
        gs.iter().all(|g| match g {
            Guard::Bool(e) => is_true(e),
            Guard::Let(_) => true,
            Guard::Bind(p, _) => matches!(p, Pat::Var(..) | Pat::Wild(_) | Pat::Lazy(_)),
        })
    })
}
