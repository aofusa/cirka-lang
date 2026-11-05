//! Linearity checking for LinearTypes (FEAT-0120, 03-type-system.md §2).
//!
//! Multiplicities are types (`FUN m a b`, ADR-037); inference records the
//! multiplicity of every non-`Many` arrow a parameter pattern is bound by
//! (`lin_pats`) and of every arrow an argument is passed to (`lin_args`).
//! This pass then walks the renamed program once more and counts, in the
//! usage semiring `0, 1, ω`, how often each local variable is used:
//!
//! * an argument's usage is scaled by the multiplicity of the arrow
//!   (constructors are linear in their fields, except `Ur`);
//! * branches of `case` / `if` / guards are joined (a linear variable must
//!   be used the same number of times in every branch);
//! * a `let` binding's right-hand side is scaled by how often the bound
//!   variable is used (GHC requires annotations here; aCHC infers, §2.3);
//! * variables bound by a pattern that matches a linearly consumed value
//!   are linear themselves.
//!
//! A linear variable used zero times is E0302, more than once E0303. A
//! `%0` variable must not be used at run time (E0303). Multiplicity
//! variables are checked as `One` (the strictest instance).

use std::collections::{HashMap, HashSet};

use achc_syntax::ast::{self, Decl, ExprKind, PatKind, Rhs, RhsBody, Stmt};
use cirka_diagnostics::{Diagnostic, Span};

use crate::infer::Tc;
use crate::types::*;

/// The usage of a variable.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Count {
    Zero,
    One,
    /// Used once on some paths and not at all on others.
    Maybe,
    Many,
}

impl Count {
    fn add(self, o: Count) -> Count {
        match (self, o) {
            (Count::Zero, x) | (x, Count::Zero) => x,
            _ => Count::Many,
        }
    }

    fn join(self, o: Count) -> Count {
        match (self, o) {
            (a, b) if a == b => a,
            (Count::Many, _) | (_, Count::Many) => Count::Many,
            _ => Count::Maybe,
        }
    }

    fn scale(self, m: Mult) -> Count {
        match (m, self) {
            (Mult::Zero, _) => Count::Zero,
            (Mult::One, c) => c,
            (Mult::Many, Count::Zero) => Count::Zero,
            (Mult::Many, _) => Count::Many,
        }
    }
}

/// A multiplicity after inference.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mult {
    Zero,
    One,
    Many,
}

/// The usage of a variable and where it is used (the first two places).
#[derive(Clone, Debug)]
struct Use {
    count: Count,
    spans: Vec<Span>,
}

type Usage = HashMap<String, Use>;

fn add(mut a: Usage, b: Usage) -> Usage {
    for (k, u) in b {
        match a.get_mut(&k) {
            Some(x) => {
                x.count = x.count.add(u.count);
                for s in u.spans {
                    if x.spans.len() < 2 {
                        x.spans.push(s);
                    }
                }
            }
            None => {
                a.insert(k, u);
            }
        }
    }
    a
}

fn scale(mut a: Usage, m: Mult) -> Usage {
    if m == Mult::Zero {
        return Usage::new();
    }
    for u in a.values_mut() {
        u.count = u.count.scale(m);
    }
    a
}

fn scale_by(a: Usage, c: Count) -> Usage {
    match c {
        Count::Zero => Usage::new(),
        Count::One => a,
        // Using a binding on some paths only: its right-hand side may run
        // zero times or once.
        Count::Maybe => join(vec![Usage::new(), a]),
        Count::Many => scale(a, Mult::Many),
    }
}

fn join(branches: Vec<Usage>) -> Usage {
    let mut keys: Vec<String> = branches.iter().flat_map(|b| b.keys().cloned()).collect();
    keys.sort();
    keys.dedup();
    let mut out = Usage::new();
    for k in keys {
        let mut count: Option<Count> = None;
        let mut spans = Vec::new();
        for b in &branches {
            let c = b.get(&k).map(|u| u.count).unwrap_or(Count::Zero);
            count = Some(count.map_or(c, |x| x.join(c)));
            if let Some(u) = b.get(&k) {
                for s in &u.spans {
                    if spans.len() < 2 {
                        spans.push(*s);
                    }
                }
            }
        }
        out.insert(
            k,
            Use {
                count: count.unwrap_or(Count::Zero),
                spans,
            },
        );
    }
    out
}

fn is_local(n: &str) -> bool {
    n.contains('#')
}

fn display(n: &str) -> &str {
    n.split('#').next().unwrap_or(n)
}

fn is_ur(q: &ast::QName) -> bool {
    q.text() == "Prelude.Linear.Ur"
}

struct Checker<'a> {
    tc: &'a Tc,
    errors: Vec<Diagnostic>,
    /// The linear variables in scope.
    linear: HashSet<String>,
}

/// Checks the linearity of the program's bindings.
pub(crate) fn check(tc: &Tc, modules: &[(String, Vec<Decl>)]) -> Vec<Diagnostic> {
    if tc.lin_pats.is_empty() {
        return Vec::new();
    }
    let mut c = Checker {
        tc,
        errors: Vec::new(),
        linear: HashSet::new(),
    };
    for (_, ds) in modules {
        for d in ds {
            c.top_decl(d);
        }
    }
    c.errors
}

impl Checker<'_> {
    fn mult(&self, t: &Ty) -> Mult {
        let t = self.tc.zonk(t);
        // An unconstrained multiplicity defaults to many (as in GHC); a
        // multiplicity variable of a signature is checked as one.
        if t == mult_many() || matches!(t, Ty::Meta(_)) {
            Mult::Many
        } else if t == mult_zero() {
            Mult::Zero
        } else {
            Mult::One
        }
    }

    fn arg_mult(&self, a: &ast::Expr) -> Mult {
        match self.tc.lin_args.get(&a.span) {
            Some(m) => self.mult(m),
            None => Mult::Many,
        }
    }

    fn top_decl(&mut self, d: &Decl) {
        match d {
            // Top-level bindings have no free locals.
            Decl::Fun(_, ps, rhs, _) => {
                self.clause(ps, rhs);
            }
            Decl::PatBind(_, rhs, _) => {
                self.rhs(rhs);
            }
            Decl::Class(c) => {
                for d in &c.decls {
                    self.top_decl(d);
                }
            }
            Decl::Instance(i) => {
                for d in &i.decls {
                    self.top_decl(d);
                }
            }
            _ => {}
        }
    }

    /// One clause: parameters bound by linear arrows are checked; the
    /// usage of the other (free) variables is returned.
    fn clause(&mut self, ps: &[ast::Pat], rhs: &Rhs) -> Usage {
        let mut bound = Vec::new();
        for p in ps {
            if let Some(m) = self.tc.lin_pats.get(&p.span) {
                let m = self.mult(m);
                if m != Mult::Many {
                    self.pat_vars(p, m, true, &mut bound);
                }
            }
        }
        self.enter(&bound);
        let u = self.rhs(rhs);
        self.close(u, &bound)
    }

    /// The variables of a pattern matched against a value of multiplicity
    /// `m` (with their multiplicity). Wildcards under a linear match drop a
    /// linear value (E0302) unless `top` is false and they are under `Ur`.
    fn pat_vars(
        &mut self,
        p: &ast::Pat,
        m: Mult,
        linear: bool,
        out: &mut Vec<(String, Mult, Span)>,
    ) {
        match &p.kind {
            PatKind::Var(v) => {
                if linear {
                    out.push((v.name.clone(), m, v.span));
                }
            }
            PatKind::Wild => {
                if linear && m == Mult::One {
                    self.errors.push(
                        Diagnostic::error("linear value is not consumed")
                            .with_code("E0302")
                            .with_label(p.span, "a wildcard discards a value bound linearly")
                            .with_note("match it with a variable and consume it, or use `consume`"),
                    );
                }
            }
            PatKind::Lit(_) | PatKind::NegLit(_) | PatKind::Error => {}
            PatKind::Con(c, ps) => {
                let lin = linear && !is_ur(c);
                for p in ps {
                    self.pat_vars(p, m, lin, out);
                }
            }
            PatKind::OpSeq(items) => {
                for i in items {
                    if let ast::OpItem::Operand(p) = i {
                        self.pat_vars(p, m, linear, out);
                    }
                }
            }
            PatKind::Tuple(ps) | PatKind::List(ps) => {
                for p in ps {
                    self.pat_vars(p, m, linear, out);
                }
            }
            PatKind::As(v, p) => {
                if linear {
                    out.push((v.name.clone(), m, v.span));
                }
                self.pat_vars(p, m, linear, out);
            }
            PatKind::Lazy(p) | PatKind::Bang(p) | PatKind::Paren(p) | PatKind::Typed(p, _) => {
                self.pat_vars(p, m, linear, out)
            }
            PatKind::Record(_, fs) => {
                for (f, p) in fs {
                    match p {
                        Some(p) => self.pat_vars(p, m, linear, out),
                        None if linear => out.push((f.name.clone(), m, f.span)),
                        None => {}
                    }
                }
            }
        }
    }

    /// Checks the variables going out of scope and removes them.
    fn close(&mut self, mut u: Usage, bound: &[(String, Mult, Span)]) -> Usage {
        for (x, m, sp) in bound {
            self.linear.remove(x);
            let used = u.remove(x);
            let count = used.as_ref().map(|u| u.count).unwrap_or(Count::Zero);
            let name = display(x);
            match (m, count) {
                (Mult::Zero, Count::Zero) | (Mult::One, Count::One) | (Mult::Many, _) => {}
                (Mult::Zero, _) => {
                    let at = used
                        .as_ref()
                        .and_then(|u| u.spans.first().copied())
                        .unwrap_or(*sp);
                    self.errors.push(
                        Diagnostic::error(format!(
                            "`{name}` has multiplicity 0 but is used at run time"
                        ))
                        .with_code("E0303")
                        .with_label(at, "used here")
                        .with_label(*sp, "bound with multiplicity 0 here"),
                    );
                }
                (Mult::One, Count::Zero) => self.errors.push(
                    Diagnostic::error(format!("linear variable `{name}` is not consumed"))
                        .with_code("E0302")
                        .with_label(*sp, "bound linearly here")
                        .with_note("a linear value must be used exactly once"),
                ),
                (Mult::One, Count::Maybe) => self.errors.push(
                    Diagnostic::error(format!(
                        "linear variable `{name}` is not consumed on every path"
                    ))
                    .with_code("E0302")
                    .with_label(*sp, "bound linearly here")
                    .with_note("every branch must use a linear value exactly once"),
                ),
                (Mult::One, Count::Many) => {
                    let mut d = Diagnostic::error(format!(
                        "linear variable `{name}` is used more than once"
                    ))
                    .with_code("E0303");
                    let spans = used.map(|u| u.spans).unwrap_or_default();
                    match spans.as_slice() {
                        [a, b, ..] => {
                            d = d
                                .with_label(*a, "first use")
                                .with_secondary(*b, "second use");
                        }
                        [a] => {
                            d = d.with_label(
                                *a,
                                "used here, in a position that may run more than once",
                            );
                        }
                        [] => {}
                    }
                    d = d
                        .with_secondary(*sp, "bound linearly here")
                        .with_note("a value passed to an unrestricted argument (`a -> b`) may be used many times");
                    self.errors.push(d)
                }
            }
        }
        u
    }

    /// Brings linearly bound variables into scope.
    fn enter(&mut self, bound: &[(String, Mult, Span)]) {
        for (x, m, _) in bound {
            if *m == Mult::One {
                self.linear.insert(x.clone());
            }
        }
    }

    fn rhs(&mut self, r: &Rhs) -> Usage {
        let body = match &r.body {
            RhsBody::Plain(e) => self.expr(e),
            RhsBody::Guarded(gs) => self.guarded(gs),
        };
        self.decls(&r.wheres, body)
    }

    fn guarded(&mut self, gs: &[ast::GuardedExpr]) -> Usage {
        let mut conds = Usage::new();
        let mut branches = Vec::new();
        for g in gs {
            let mut bound = Vec::new();
            let mut gu = Usage::new();
            for s in &g.guards {
                gu = add(gu, self.guard_stmt(s, &mut bound));
            }
            self.enter(&bound);
            let b = self.expr(&g.body);
            let b = self.close(b, &bound);
            conds = add(conds, gu);
            branches.push(b);
        }
        add(conds, join(branches))
    }

    fn guard_stmt(&mut self, s: &Stmt, bound: &mut Vec<(String, Mult, Span)>) -> Usage {
        match s {
            Stmt::Expr(e) => self.expr(e),
            Stmt::Bind(p, e, _) => {
                let u = self.expr(e);
                if self.consumes_linear(&u) {
                    let start = bound.len();
                    self.pat_vars(p, Mult::One, true, bound);
                    let new = bound[start..].to_vec();
                    self.enter(&new);
                }
                u
            }
            Stmt::Let(ds, _) => {
                // Bindings of a guard `let` scope over the rest; count them
                // unrestricted (conservative).
                let mut u = Usage::new();
                for d in ds {
                    u = add(u, scale(self.decl_rhs(d), Mult::Many));
                }
                u
            }
        }
    }

    /// `let`/`where` bindings around a body: each right-hand side is scaled
    /// by the usage of what it binds (03-type-system.md §2.3).
    fn decls(&mut self, ds: &[Decl], body: Usage) -> Usage {
        if ds.is_empty() {
            return body;
        }
        let mut rhss: Vec<(Vec<String>, Usage)> = Vec::new();
        let mut funs: HashMap<String, usize> = HashMap::new();
        for d in ds {
            match d {
                Decl::Fun(f, ..) => {
                    let u = self.decl_rhs(d);
                    match funs.get(&f.name) {
                        // Further clauses of the same function: one of them
                        // runs per call.
                        Some(&i) => {
                            let prev = std::mem::take(&mut rhss[i].1);
                            rhss[i].1 = join(vec![prev, u]);
                        }
                        None => {
                            funs.insert(f.name.clone(), rhss.len());
                            rhss.push((vec![f.name.clone()], u));
                        }
                    }
                }
                Decl::PatBind(p, ..) => {
                    let mut vs = Vec::new();
                    self.pat_vars(p, Mult::Many, true, &mut vs);
                    let u = self.decl_rhs(d);
                    rhss.push((vs.into_iter().map(|v| v.0).collect(), u));
                }
                _ => {}
            }
        }
        // Resolve the bound names' usage, innermost first: the body and the
        // other right-hand sides may use them.
        let mut total = body;
        let names: Vec<String> = rhss.iter().flat_map(|r| r.0.clone()).collect();
        let mut pending = rhss;
        // A binding's right-hand side may use bindings that come before or
        // after it; iterate until every binding is accounted for, treating
        // recursion as unrestricted use.
        let mut guard = pending.len() + 1;
        while !pending.is_empty() && guard > 0 {
            guard -= 1;
            // Bindings no other pending right-hand side refers to.
            let idx = (0..pending.len()).find(|&i| {
                let mine = &pending[i].0;
                pending
                    .iter()
                    .enumerate()
                    .all(|(j, (_, u))| j == i || mine.iter().all(|n| !u.contains_key(n)))
            });
            let i = match idx {
                Some(i) => i,
                None => break,
            };
            let (vs, u) = pending.remove(i);
            let c = vs
                .iter()
                .map(|v| total.remove(v).map(|u| u.count).unwrap_or(Count::Zero))
                .fold(Count::Zero, Count::add);
            let mut u = u;
            for v in &vs {
                if u.remove(v).is_some() {
                    // Recursive: may run any number of times.
                    u = scale(u, Mult::Many);
                }
            }
            total = add(total, scale_by(u, c));
        }
        // Mutually recursive groups: unrestricted.
        for (vs, u) in pending {
            let mut u = u;
            for v in &vs {
                u.remove(v);
            }
            total = add(total, scale(u, Mult::Many));
        }
        for n in names {
            total.remove(&n);
        }
        total
    }

    fn decl_rhs(&mut self, d: &Decl) -> Usage {
        match d {
            // A function's free variables are used once per call; the
            // caller scales by the usage of the function's name.
            Decl::Fun(_, ps, rhs, _) => self.clause(ps, rhs),
            Decl::PatBind(_, rhs, _) => {
                // A lazy pattern binding may be demanded through any of its
                // variables: unrestricted.
                let u = self.rhs(rhs);
                scale(u, Mult::Many)
            }
            _ => Usage::new(),
        }
    }

    fn var(&self, q: &ast::QName) -> Usage {
        let mut u = Usage::new();
        if q.qual.is_none() && is_local(&q.name) {
            u.insert(
                q.name.clone(),
                Use {
                    count: Count::One,
                    spans: vec![q.span],
                },
            );
        }
        u
    }

    fn expr(&mut self, e: &ast::Expr) -> Usage {
        match &e.kind {
            ExprKind::Var(q) => self.var(q),
            ExprKind::Con(_) | ExprKind::Lit(_) | ExprKind::Error => Usage::new(),
            ExprKind::App(f, a) => {
                let fu = self.expr(f);
                let m = match head_con(f) {
                    Some(c) if is_ur(c) => Mult::Many,
                    Some(_) => Mult::One,
                    None => self.arg_mult(a),
                };
                let au = self.expr(a);
                add(fu, scale(au, m))
            }
            ExprKind::OpSeq(items) => {
                let mut u = Usage::new();
                for i in items {
                    match i {
                        ast::OpItem::Operand(x) => u = add(u, scale(self.expr(x), Mult::Many)),
                        ast::OpItem::Op(q) => u = add(u, self.var(q)),
                        _ => {}
                    }
                }
                u
            }
            ExprKind::Lam(ps, b) => {
                let mut bound = Vec::new();
                for p in ps {
                    if let Some(m) = self.tc.lin_pats.get(&p.span) {
                        let m = self.mult(m);
                        if m != Mult::Many {
                            self.pat_vars(p, m, true, &mut bound);
                        }
                    }
                }
                self.enter(&bound);
                let u = self.expr(b);
                self.close(u, &bound)
            }
            ExprKind::Let(ds, b) => {
                let bu = self.expr(b);
                self.decls(ds, bu)
            }
            ExprKind::If(c, t, f) => {
                let cu = self.expr(c);
                let tu = self.expr(t);
                let fu = self.expr(f);
                add(cu, join(vec![tu, fu]))
            }
            ExprKind::MultiIf(gs) => self.guarded(gs),
            ExprKind::Case(s, alts) => {
                let su = self.expr(s);
                let lin = self.consumes_linear(&su);
                let bs = alts.iter().map(|a| self.alt(a, lin)).collect();
                add(su, join(bs))
            }
            ExprKind::LamCase(alts) => {
                // `\case` binds an argument of the arrow's multiplicity.
                let lin = self
                    .tc
                    .lin_pats
                    .get(&e.span)
                    .is_some_and(|m| self.mult(m) == Mult::One);
                let bs = alts.iter().map(|a| self.alt(a, lin)).collect();
                join(bs)
            }
            ExprKind::Do(ss) => {
                // `>>=` / `>>` of the standard monads are unrestricted.
                let mut u = Usage::new();
                for s in ss {
                    u = add(u, self.stmt_unrestricted(s));
                }
                scale(u, Mult::Many)
            }
            ExprKind::Tuple(es) | ExprKind::List(es) => {
                let mut u = Usage::new();
                for x in es {
                    u = add(u, self.expr(x));
                }
                u
            }
            ExprKind::TupleSection(es) => {
                let mut u = Usage::new();
                for x in es.iter().flatten() {
                    u = add(u, self.expr(x));
                }
                u
            }
            ExprKind::Range(a, b, c) => {
                let mut u = self.expr(a);
                for x in [b, c].into_iter().flatten() {
                    u = add(u, self.expr(x));
                }
                scale(u, Mult::Many)
            }
            ExprKind::ListComp(h, qs) => {
                let mut u = self.expr(h);
                for s in qs {
                    u = add(u, self.stmt_unrestricted(s));
                }
                scale(u, Mult::Many)
            }
            ExprKind::LeftSection(x, op) | ExprKind::RightSection(op, x) => {
                let xu = scale(self.expr(x), Mult::Many);
                add(self.var(op), xu)
            }
            ExprKind::Typed(x, _) | ExprKind::Paren(x) => self.expr(x),
            ExprKind::RecordCon(_, fs) => {
                let mut u = Usage::new();
                for (f, x) in fs {
                    match x {
                        Some(x) => u = add(u, self.expr(x)),
                        None => u = add(u, self.var(f)),
                    }
                }
                u
            }
            ExprKind::RecordUpd(x, fs) => {
                let mut u = scale(self.expr(x), Mult::Many);
                for (_, x) in fs {
                    u = add(u, self.expr(x));
                }
                u
            }
        }
    }

    fn alt(&mut self, a: &ast::Alt, linear: bool) -> Usage {
        let mut bound = Vec::new();
        if linear {
            self.pat_vars(&a.pat, Mult::One, true, &mut bound);
        }
        self.enter(&bound);
        let u = self.rhs(&a.rhs);
        self.close(u, &bound)
    }

    /// Whether a scrutinee's usage consumes a linear variable: then the
    /// variables bound by its patterns are linear too.
    fn consumes_linear(&self, u: &Usage) -> bool {
        u.iter()
            .any(|(x, n)| n.count == Count::One && self.linear.contains(x))
    }

    fn stmt_unrestricted(&mut self, s: &Stmt) -> Usage {
        match s {
            Stmt::Expr(e) | Stmt::Bind(_, e, _) => self.expr(e),
            Stmt::Let(ds, _) => {
                let mut u = Usage::new();
                for d in ds {
                    u = add(u, self.decl_rhs(d));
                }
                u
            }
        }
    }
}

/// The constructor at the head of an application spine.
fn head_con(e: &ast::Expr) -> Option<&ast::QName> {
    match &e.kind {
        ExprKind::Con(q) => Some(q),
        ExprKind::App(f, _) => head_con(f),
        ExprKind::Paren(x) => head_con(x),
        _ => None,
    }
}
