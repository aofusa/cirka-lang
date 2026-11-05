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
    tc: &'a mut Tc,
    errors: Vec<Diagnostic>,
    /// The linear variables in scope.
    linear: HashSet<String>,
    /// Those of them whose type is linear-only (`LType`), with the span of
    /// their binder.
    unique: HashMap<String, Span>,
    plan: Plan,
    /// Instances of `Drop` / `Release` found for types (by their text).
    has_instance: HashMap<(&'static str, String), bool>,
}

/// Where a linear-only value left unused is given up (FEAT-0122,
/// 03-type-system.md §3).
#[derive(Clone, Debug)]
enum Scope {
    /// At the start of these (marked) expressions.
    Exprs(Vec<Span>),
    /// After the last statement (this expression) of an `IO` `do` block.
    DoEnd(Span),
}

pub(crate) const DROP: &str = "Cirka.Ownership.Drop";
pub(crate) const RELEASE: &str = "Cirka.Ownership.Release";

/// An inserted `drop x` / `release x`.
#[derive(Clone, Debug)]
pub(crate) struct Insertion {
    /// The marked expression.
    pub span: Span,
    /// After the expression (an `IO` action) instead of before it.
    pub after: bool,
    pub release: bool,
    pub var: String,
    pub ty: Ty,
}

/// What the checker found besides errors: insertions and the `let`
/// bindings to evaluate strictly (Strict-Linear, §6).
#[derive(Clone, Debug, Default)]
pub(crate) struct Plan {
    pub insertions: Vec<Insertion>,
    pub strict_lets: HashSet<String>,
}

/// Checks the linearity of the program's bindings.
pub(crate) fn check(
    tc: &mut Tc,
    modules: &[(String, Vec<Decl>)],
    files: &HashMap<String, cirka_diagnostics::FileId>,
) -> (Vec<Diagnostic>, Plan) {
    if tc.lin_pats.is_empty() && !tc.has_ltype {
        return (Vec::new(), Plan::default());
    }
    let mut c = Checker {
        tc,
        errors: Vec::new(),
        linear: HashSet::new(),
        unique: HashMap::new(),
        plan: Plan::default(),
        has_instance: HashMap::new(),
    };
    // `Unsafe.*` modules implement the escape hatches: not checked.
    let unchecked: HashSet<cirka_diagnostics::FileId> = files
        .iter()
        .filter(|(m, _)| m.starts_with("Unsafe."))
        .map(|(_, f)| *f)
        .collect();
    for (m, ds) in modules {
        if m.starts_with("Unsafe.") {
            continue;
        }
        for d in ds {
            c.top_decl(d);
        }
    }
    c.uniqueness();
    c.errors.retain(|d| {
        d.primary_span()
            .is_none_or(|s| !unchecked.contains(&s.file))
    });
    (c.errors, c.plan)
}

/// The spans of the bodies of a right-hand side (marked by inference).
fn rhs_spans(r: &Rhs) -> Vec<Span> {
    match &r.body {
        RhsBody::Plain(e) => vec![e.span],
        RhsBody::Guarded(gs) => gs.iter().map(|g| g.body.span).collect(),
    }
}

impl Checker<'_> {
    /// Whether the pattern or binder at `sp` has a linear-only type.
    fn ltype_at(&self, sp: Span) -> bool {
        self.tc
            .pat_tys
            .get(&sp)
            .is_some_and(|t| self.tc.is_ltype(t))
    }

    fn show_at(&self, sp: Span) -> String {
        self.tc
            .pat_tys
            .get(&sp)
            .map(|t| show(&self.tc.zonk(t)))
            .unwrap_or_default()
    }

    /// U-Bind: a linear-only value bound with multiplicity ω (E0301).
    fn omega_binder(&mut self, sp: Span) {
        let t = self.show_at(sp);
        self.errors.push(
            Diagnostic::error(format!(
                "a value of the linear-only type `{t}` is bound with multiplicity ω"
            ))
            .with_code("E0301")
            .with_label(sp, "bound by an unrestricted arrow (`->`)")
            .with_help("use a linear arrow (`%1 ->`) for this argument"),
        );
    }

    /// E0301 for arguments and E0310 for instantiations (03-type-system.md
    /// §2.2): linear-only values must not reach unrestricted positions.
    fn uniqueness(&mut self) {
        let tc = &*self.tc;
        let mut errors = Vec::new();
        let mut seen: HashSet<Span> = HashSet::new();
        for (sp, name, v, t) in &tc.inst_checks {
            if tc.is_ltype(t) && seen.insert(*sp) {
                errors.push(
                    Diagnostic::error(format!(
                        "the type variable `{}` of `{}` has kind `Type`, but is instantiated with the linear-only type `{}`",
                        display(v),
                        short(name),
                        show(&tc.zonk(t))
                    ))
                    .with_code("E0310")
                    .with_label(*sp, "used here")
                    .with_help(
                        "a polymorphic function may copy values of kind `Type`; declare the variable `forall (a :: LType)` if it is used linearly",
                    ),
                );
            }
        }
        for (sp, t) in &tc.omega_args {
            if tc.is_ltype(t) {
                errors.push(
                    Diagnostic::error(format!(
                        "a value of the linear-only type `{}` is passed to an unrestricted argument",
                        show(&tc.zonk(t))
                    ))
                    .with_code("E0301")
                    .with_label(*sp, "this argument may be used many times")
                    .with_help("the function must take this argument with a linear arrow (`%1 ->`)"),
                );
            }
        }
        self.errors.extend(errors);
    }

    /// Whether `Drop` / `Release` has an instance for a type.
    fn instance(&mut self, class: &'static str, t: &Ty) -> bool {
        let t = self.tc.zonk(t);
        let key = (class, show(&t));
        if let Some(&b) = self.has_instance.get(&key) {
            return b;
        }
        let b = self.tc.closed_instance(class, &t).is_some();
        self.has_instance.insert(key, b);
        b
    }

    /// A linear-only variable not used in its scope: inserts `drop` (or,
    /// at the end of an `IO` block, `release`) when the type has an
    /// instance (Weaken-Drop / Weaken-Release). False when it must be an
    /// error E0302.
    fn weaken(&mut self, x: &str, sp: Span, scope: &Scope) -> bool {
        let Some(t) = self.tc.pat_tys.get(&sp).cloned() else {
            return false;
        };
        let t = self.tc.zonk(&t);
        if self.instance(DROP, &t) {
            match scope {
                Scope::Exprs(spans) if !spans.is_empty() => {
                    for s in spans {
                        self.plan.insertions.push(Insertion {
                            span: *s,
                            after: false,
                            release: false,
                            var: x.to_string(),
                            ty: t.clone(),
                        });
                    }
                    true
                }
                Scope::DoEnd(s) => {
                    self.plan.insertions.push(Insertion {
                        span: *s,
                        after: true,
                        release: false,
                        var: x.to_string(),
                        ty: t,
                    });
                    true
                }
                Scope::Exprs(_) => false,
            }
        } else if self.instance(RELEASE, &t) {
            match scope {
                Scope::DoEnd(s) => {
                    self.plan.insertions.push(Insertion {
                        span: *s,
                        after: true,
                        release: true,
                        var: x.to_string(),
                        ty: t,
                    });
                }
                Scope::Exprs(_) => self.errors.push(
                    Diagnostic::error(format!(
                        "`{}` of type `{}` can only be released in `IO`, but is discarded in a pure context",
                        display(x),
                        show(&t)
                    ))
                    .with_code("E0304")
                    .with_label(sp, "bound here and not consumed")
                    .with_note("the type has a `Release` instance but no `Drop` instance")
                    .with_help("consume it explicitly, or bind it in an `IO` `do` block, whose end releases it"),
                ),
            }
            true
        } else {
            false
        }
    }

    /// Joins the branches of a conditional. A linear-only variable used in
    /// some branches and not in others is dropped at the start of the
    /// others when its type has a `Drop` instance (the earliest point,
    /// 03-type-system.md §3).
    fn join_branches(&mut self, mut branches: Vec<(Usage, Vec<Span>)>) -> Usage {
        let vars: Vec<(String, Span)> = self.unique.iter().map(|(k, v)| (k.clone(), *v)).collect();
        for (x, sp) in vars {
            let counts: Vec<Count> = branches
                .iter()
                .map(|(u, _)| u.get(&x).map(|u| u.count).unwrap_or(Count::Zero))
                .collect();
            if !(counts.contains(&Count::One) && counts.contains(&Count::Zero))
                || counts
                    .iter()
                    .any(|c| matches!(c, Count::Many | Count::Maybe))
            {
                continue;
            }
            let Some(t) = self.tc.pat_tys.get(&sp).cloned() else {
                continue;
            };
            if !self.instance(DROP, &t) {
                continue;
            }
            let t = self.tc.zonk(&t);
            for (u, spans) in branches.iter_mut() {
                if u.contains_key(&x) || spans.is_empty() {
                    continue;
                }
                for s in spans.iter() {
                    self.plan.insertions.push(Insertion {
                        span: *s,
                        after: false,
                        release: false,
                        var: x.clone(),
                        ty: t.clone(),
                    });
                }
                u.insert(
                    x.clone(),
                    Use {
                        count: Count::One,
                        spans: Vec::new(),
                    },
                );
            }
        }
        join(branches.into_iter().map(|(u, _)| u).collect())
    }

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
            Decl::Fun(f, ps, rhs, _) => {
                if ps.is_empty() && self.ltype_at(f.span) {
                    let t = self.show_at(f.span);
                    self.errors.push(
                        Diagnostic::error(format!(
                            "a top-level binding cannot have the linear-only type `{t}`"
                        ))
                        .with_code("E0301")
                        .with_label(f.span, "a top-level value may be used many times")
                        .with_help("make it a function, e.g. of `()`, that creates the value"),
                    );
                }
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
        let bound = self.params(ps);
        self.enter(&bound);
        let u = self.rhs(rhs);
        self.close(u, &bound, &Scope::Exprs(rhs_spans(rhs)))
    }

    /// The variables bound by the parameters of a clause or lambda.
    fn params(&mut self, ps: &[ast::Pat]) -> Vec<(String, Mult, Span)> {
        let mut bound = Vec::new();
        for p in ps {
            let m = match self.tc.lin_pats.get(&p.span) {
                Some(m) => self.mult(m),
                None => Mult::Many,
            };
            if m != Mult::Many {
                self.pat_vars(p, m, true, &mut bound);
            } else if self.ltype_at(p.span) {
                self.omega_binder(p.span);
            } else {
                self.pat_vars(p, m, false, &mut bound);
            }
        }
        bound
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
                } else if self.ltype_at(p.span) {
                    out.push((v.name.clone(), Mult::One, v.span));
                }
            }
            PatKind::Wild => {
                if (linear && m == Mult::One) || self.ltype_at(p.span) {
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
            PatKind::As(v, inner) => {
                if linear {
                    out.push((v.name.clone(), m, v.span));
                } else if self.ltype_at(p.span) {
                    out.push((v.name.clone(), Mult::One, v.span));
                }
                let p = inner;
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
    fn close(&mut self, mut u: Usage, bound: &[(String, Mult, Span)], scope: &Scope) -> Usage {
        for (x, m, sp) in bound {
            self.linear.remove(x);
            self.unique.remove(x);
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
                (Mult::One, Count::Zero) if self.weaken(x, *sp, scope) => {}
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
                            d = d
                                .with_label(
                                    *a,
                                    "used here, in a position that may run more than once",
                                )
                                .with_note(
                                    "an unrestricted argument (`a -> b`), a `let` binding used several times or a recursive binding may run many times",
                                );
                        }
                        [] => {}
                    }
                    d = d.with_secondary(*sp, "bound linearly here");
                    self.errors.push(d)
                }
            }
        }
        u
    }

    /// Brings linearly bound variables into scope.
    fn enter(&mut self, bound: &[(String, Mult, Span)]) {
        for (x, m, sp) in bound {
            if *m == Mult::One {
                self.linear.insert(x.clone());
                if self.ltype_at(*sp) {
                    self.unique.insert(x.clone(), *sp);
                }
            }
        }
    }

    /// Brings the linear-only names bound by `let`/`where` declarations
    /// into scope before the body is checked (`decls` closes them).
    fn enter_let(&mut self, ds: &[Decl]) {
        let mut bound = Vec::new();
        for d in ds {
            match d {
                Decl::Fun(f, ps, ..) if ps.is_empty() && self.ltype_at(f.span) => {
                    bound.push((f.name.clone(), Mult::One, f.span));
                }
                Decl::PatBind(p, ..) => {
                    let mut errs = std::mem::take(&mut self.errors);
                    self.pat_vars(p, Mult::Many, false, &mut bound);
                    // Reported again by `decls`.
                    std::mem::swap(&mut errs, &mut self.errors);
                }
                _ => {}
            }
        }
        self.enter(&bound);
    }

    fn rhs(&mut self, r: &Rhs) -> Usage {
        self.enter_let(&r.wheres);
        let body = match &r.body {
            RhsBody::Plain(e) => self.expr(e),
            RhsBody::Guarded(gs) => self.guarded(gs),
        };
        self.decls(&r.wheres, body, &Scope::Exprs(rhs_spans(r)))
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
            let b = self.close(b, &bound, &Scope::Exprs(vec![g.body.span]));
            conds = add(conds, gu);
            branches.push((b, vec![g.body.span]));
        }
        let joined = self.join_branches(branches);
        add(conds, joined)
    }

    fn guard_stmt(&mut self, s: &Stmt, bound: &mut Vec<(String, Mult, Span)>) -> Usage {
        match s {
            Stmt::Expr(e) => self.expr(e),
            Stmt::Bind(p, e, _) => {
                let u = self.expr(e);
                let lin = self.consumes_linear(&u);
                let start = bound.len();
                self.pat_vars(p, Mult::One, lin, bound);
                let new = bound[start..].to_vec();
                self.enter(&new);
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
    fn decls(&mut self, ds: &[Decl], body: Usage, scope: &Scope) -> Usage {
        if ds.is_empty() {
            return body;
        }
        let mut rhss: Vec<(Vec<String>, Usage)> = Vec::new();
        let mut funs: HashMap<String, usize> = HashMap::new();
        // Let-bound names of linear-only types must be used exactly once.
        let mut lin_binders: Vec<(String, Mult, Span)> = Vec::new();
        for d in ds {
            match d {
                Decl::Fun(f, ps, ..) => {
                    if ps.is_empty() && !funs.contains_key(&f.name) && self.ltype_at(f.span) {
                        lin_binders.push((f.name.clone(), Mult::One, f.span));
                    }
                    let u = self.decl_rhs(d);
                    // Strict-Linear (§6): a `let` whose right-hand side uses
                    // a linear variable is evaluated where it is bound.
                    if ps.is_empty()
                        && u.iter()
                            .any(|(k, x)| self.linear.contains(k) && x.count != Count::Zero)
                    {
                        self.plan.strict_lets.insert(f.name.clone());
                    }
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
                    let mut lvs = Vec::new();
                    self.pat_vars(p, Mult::Many, false, &mut lvs);
                    lin_binders.extend(lvs);
                    let mut vs = Vec::new();
                    pat_names(p, &mut vs);
                    let u = self.decl_rhs(d);
                    rhss.push((vs, u));
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
            let mut c = Count::Zero;
            for v in &vs {
                let used = total.remove(v);
                c = c.add(used.as_ref().map(|u| u.count).unwrap_or(Count::Zero));
                if let Some(b) = lin_binders.iter().find(|b| b.0 == *v).cloned() {
                    let mut one = Usage::new();
                    if let Some(used) = used {
                        one.insert(v.clone(), used);
                    }
                    self.close(one, &[b], scope);
                }
            }
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
                add(fu, self.scale_arg(au, m, a.span))
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
                let bound = self.params(ps);
                self.enter(&bound);
                let u = self.expr(b);
                self.close(u, &bound, &Scope::Exprs(vec![b.span]))
            }
            ExprKind::Let(ds, b) => {
                self.enter_let(ds);
                let bu = self.expr(b);
                self.decls(ds, bu, &Scope::Exprs(vec![b.span]))
            }
            ExprKind::If(c, t, f) => {
                let cu = self.expr(c);
                let tu = self.expr(t);
                let fu = self.expr(f);
                let joined = self.join_branches(vec![(tu, vec![t.span]), (fu, vec![f.span])]);
                add(cu, joined)
            }
            ExprKind::MultiIf(gs) => self.guarded(gs),
            ExprKind::Case(s, alts) => {
                let su = self.expr(s);
                let lin = self.consumes_linear(&su);
                let bs = alts
                    .iter()
                    .map(|a| (self.alt(a, lin), rhs_spans(&a.rhs)))
                    .collect();
                let joined = self.join_branches(bs);
                add(su, joined)
            }
            ExprKind::LamCase(alts) => {
                // `\case` binds an argument of the arrow's multiplicity.
                let lin = self
                    .tc
                    .lin_pats
                    .get(&e.span)
                    .is_some_and(|m| self.mult(m) == Mult::One);
                let bs = alts
                    .iter()
                    .map(|a| (self.alt(a, lin), rhs_spans(&a.rhs)))
                    .collect();
                self.join_branches(bs)
            }
            ExprKind::Do(ss) => {
                // The statements of an `IO` block run once, in order
                // (FEAT-0122 releases resources at its end); `>>=` / `>>`
                // of the other monads are unrestricted.
                let io = self.tc.do_tys.get(&e.span).is_some_and(
                    |t| matches!(self.tc.zonk(t).spine().0, Ty::Con(c) if *c == prim("IO")),
                );
                if io {
                    let last = match ss.last() {
                        Some(Stmt::Expr(l)) => l.span,
                        _ => e.span,
                    };
                    return self.io_stmts(ss, last);
                }
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

    /// Scales an argument's usage by its arrow's multiplicity. A
    /// first-order argument (no closure inside, not linear-only) of an
    /// unrestricted arrow is a thunk evaluated at most once that cannot
    /// carry a reference to a linear-only value: the linear-only variables
    /// it uses keep their count (U-Bind is checked on the argument's type,
    /// E0301). Other linear variables follow Linear Haskell.
    fn scale_arg(&self, mut u: Usage, m: Mult, sp: Span) -> Usage {
        if m != Mult::Many || !u.keys().any(|k| self.unique.contains_key(k)) {
            return scale(u, m);
        }
        let first_order = self
            .tc
            .arg_tys
            .get(&sp)
            .is_some_and(|t| !self.tc.may_hold_closure(t) && !self.tc.is_ltype(t));
        if !first_order {
            return scale(u, m);
        }
        for (k, x) in u.iter_mut() {
            if !self.unique.contains_key(k) {
                x.count = x.count.scale(m);
            }
        }
        u
    }

    fn alt(&mut self, a: &ast::Alt, linear: bool) -> Usage {
        let mut bound = Vec::new();
        self.pat_vars(&a.pat, Mult::One, linear, &mut bound);
        self.enter(&bound);
        let u = self.rhs(&a.rhs);
        self.close(u, &bound, &Scope::Exprs(rhs_spans(&a.rhs)))
    }

    /// Whether a scrutinee's usage consumes a linear variable: then the
    /// variables bound by its patterns are linear too.
    fn consumes_linear(&self, u: &Usage) -> bool {
        u.iter()
            .any(|(x, n)| n.count == Count::One && self.linear.contains(x))
    }

    /// The statements of an `IO` `do` block, each run once.
    fn io_stmts(&mut self, ss: &[Stmt], last: Span) -> Usage {
        let Some((s, rest)) = ss.split_first() else {
            return Usage::new();
        };
        match s {
            Stmt::Expr(e) => {
                let u = self.expr(e);
                let r = self.io_stmts(rest, last);
                add(u, r)
            }
            Stmt::Bind(p, e, _) => {
                let u = self.expr(e);
                let mut bound = Vec::new();
                self.pat_vars(p, Mult::One, false, &mut bound);
                self.enter(&bound);
                let r = self.io_stmts(rest, last);
                let r = self.close(r, &bound, &Scope::DoEnd(last));
                add(u, r)
            }
            Stmt::Let(ds, _) => {
                self.enter_let(ds);
                let r = self.io_stmts(rest, last);
                self.decls(ds, r, &Scope::DoEnd(last))
            }
        }
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

/// The variables a pattern binds.
fn pat_names(p: &ast::Pat, out: &mut Vec<String>) {
    match &p.kind {
        PatKind::Var(v) => out.push(v.name.clone()),
        PatKind::As(v, p) => {
            out.push(v.name.clone());
            pat_names(p, out);
        }
        PatKind::Con(_, ps) | PatKind::Tuple(ps) | PatKind::List(ps) => {
            for p in ps {
                pat_names(p, out);
            }
        }
        PatKind::OpSeq(items) => {
            for i in items {
                if let ast::OpItem::Operand(p) = i {
                    pat_names(p, out);
                }
            }
        }
        PatKind::Lazy(p) | PatKind::Bang(p) | PatKind::Paren(p) | PatKind::Typed(p, _) => {
            pat_names(p, out)
        }
        PatKind::Record(_, fs) => {
            for (f, p) in fs {
                match p {
                    Some(p) => pat_names(p, out),
                    None => out.push(f.name.clone()),
                }
            }
        }
        PatKind::Wild | PatKind::Lit(_) | PatKind::NegLit(_) | PatKind::Error => {}
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
