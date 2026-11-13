//! Bidirectional elaboration of the resolved program into TT, with
//! multiplicity (QTT) checking, universes, data declarations and the
//! compilation of clauses into case trees (spec-v0.0.1.md §4).

use crate::eval::*;
use crate::tt::*;
use cirka_diagnostics::{Diagnostic, Span};
use cirkalang_resolve::rast::{self, ExprKind, PatKind};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod raii;
mod traits;
use traits::{Goal, Pending, Search};

type SLit = cirkalang_resolve::rast::Lit;

#[derive(Clone, Debug, Default)]
struct Usage {
    m: Option<Mult>,
    spans: Vec<Span>,
}

impl Usage {
    fn mult(&self) -> Mult {
        self.m.unwrap_or(Mult::Zero)
    }
}

/// The elaboration context.
#[derive(Clone, Default)]
struct Cxt {
    env: Env,
    names: Vec<Name>,
    /// Type of each variable, in the context of the variables before it.
    tys: Vec<Term>,
    mults: Vec<Mult>,
    /// Bound (abstracted by inserted metas) rather than defined.
    bound: Vec<bool>,
    spans: Vec<Span>,
    /// Source names in scope.
    scope: Vec<(String, u32)>,
    uses: Vec<Usage>,
    /// Expressions of the current scope whose value may hold a linear value
    /// that its type does not show (spec-v0.0.2.md §5.1).
    holders: Vec<Span>,
    /// Binders that are linear because their value holds the linear value
    /// created at the span (for messages).
    holds: HashMap<u32, Span>,
    /// Shapes learnt by splitting (for "missing case" examples).
    shapes: HashMap<u32, Shape>,
}

/// What `enter_scope` saves.
struct Scope {
    uses: Vec<Usage>,
    holders: Vec<Span>,
}

#[derive(Clone)]
enum Shape {
    Con(String, Vec<u32>),
    Lit(String),
}

impl Cxt {
    fn lvl(&self) -> u32 {
        self.env.len() as u32
    }

    fn lookup(&self, x: &str) -> Option<u32> {
        self.scope.iter().rev().find(|(n, _)| n == x).map(|p| p.1)
    }

    fn ix(&self, l: u32) -> u32 {
        self.lvl() - 1 - l
    }
}

#[derive(Clone)]
struct Row {
    pats: Vec<(u32, rast::Pat)>,
    binds: Vec<(String, u32)>,
    rhs: Rc<rast::Expr>,
    clause: usize,
}

struct ClauseInfo {
    what: String,
    used: Vec<bool>,
    spans: Vec<Span>,
    top: Vec<u32>,
    span: Span,
}

enum Unif {
    Ok,
    Absurd,
    Stuck(String),
}

/// How a linear value can be given up (spec-v0.0.2.md §5.3), ordered
/// from the least to the most demanding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DropStatus {
    /// Only memory: Perceus frees it.
    Free,
    /// It holds a value with a `Drop` or `Release` implementation.
    Fin,
    /// It must be consumed explicitly (E0302).
    Stuck,
}

/// The finalizer of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fin {
    Drop,
    Release,
}

pub struct Elab<'r> {
    rp: &'r rast::Program,
    pub prog: Program,
    metas: Metas,
    meta_spans: RefCell<Vec<Span>>,
    meta_reported: RefCell<Vec<bool>>,
    fuel: Cell<u64>,
    out_of_fuel: Cell<bool>,
    fuel_span: Cell<Option<Span>>,
    pub diags: RefCell<Vec<Diagnostic>>,
    /// Name of the definition being elaborated (for messages).
    current: RefCell<String>,
    /// Constraints to solve when more types are known.
    pending: RefCell<Vec<Pending>>,
    /// Sort metas checked against `LType` become `LType` (the variables of
    /// an `impl` of a trait over `LType`) instead of `Type`.
    ltype_vars: Cell<bool>,
    /// Checks that wait until the metas of the definition are solved
    /// (spec-v0.0.2.md §5.1): `(cx, type, sort, span, what)` must be
    /// shareable.
    shareable: RefCell<Vec<(Cxt, Val, Val, Span, &'static str)>>,
    /// Linear binders of the definition that are left unused on some path
    /// and may need a finalizer (name, span), for the RAII pass.
    fin_spans: RefCell<Vec<(Name, Span)>>,
    /// The span of the definition, for finalizer errors on binders that
    /// `fin_spans` does not locate.
    fin_span_default: Cell<Span>,
    /// The spans of held linear values that have a finalizer: a holder of
    /// one that is left unused leaks it (W0308).
    fin_held: RefCell<HashSet<Span>>,
}

fn sp(e: &rast::Expr) -> Span {
    e.span
}

fn mk(kind: ExprKind, span: Span) -> rast::Expr {
    rast::Expr { kind, span }
}

fn app(f: rast::Expr, a: rast::Expr) -> rast::Expr {
    let span = f.span.to(a.span);
    mk(ExprKind::App(Box::new(f), Box::new(a)), span)
}

impl<'r> Elab<'r> {
    pub fn new(rp: &'r rast::Program) -> Elab<'r> {
        let mut prog = Program {
            globals: vec![Global::Missing; rp.globals.len()],
            names: rp.globals.iter().map(|g| g.qualified()).collect(),
            main: rp.main,
            order: Vec::new(),
            builtins: Builtins::default(),
            traits: HashMap::new(),
            impls: HashMap::new(),
        };
        let f = |n: &str| rp.find("Prelude", n);
        prog.builtins = Builtins {
            bool_: f("Bool"),
            true_: f("True"),
            false_: f("False"),
            unit: f("Unit"),
            mk_unit: f("MkUnit"),
            pair: f("Pair"),
            mk_pair: f("MkPair"),
            list: f("List"),
            nil: f("Nil"),
            cons: f("::"),
            nat: f("Nat"),
            zero: f("Z"),
            succ: f("S"),
            bind: f(">>="),
            prim_bind: f("primBind"),
            then: f(">>"),
            num: f("Num"),
            from_int: f("fromInt"),
            negate: f("negate"),
            int: f("Int"),
            uint: f("UInt"),
            float: f("Float"),
            char_: f("Char"),
            string: f("String"),
            io: f("IO"),
            unique: f("Unique"),
            io1: f("IO1"),
            prim_pure: f("primPure"),
            prim_pure1: f("primPure1"),
            prim_bind1: f("primBind1"),
            prim_bind_to1: f("primBindTo1"),
            prim_bind11: f("primBind11"),
            drop_trait: f("Drop"),
            release_trait: f("Release"),
            drop_method: f("drop"),
            release_method: f("release"),
            drop_free: f("dropFree"),
            release_free: f("releaseFree"),
            release_by_drop: f("releaseByDrop"),
            drop_after: f("dropAfter"),
            release_after: f("releaseAfter"),
            drop_after1: f("dropAfter1"),
            release_after1: f("releaseAfter1"),
            result: f("Result"),
            ok: f("Ok"),
            err: f("Err"),
        };
        Elab {
            rp,
            prog,
            metas: Metas::default(),
            meta_spans: RefCell::new(Vec::new()),
            meta_reported: RefCell::new(Vec::new()),
            fuel: Cell::new(FUEL),
            out_of_fuel: Cell::new(false),
            fuel_span: Cell::new(None),
            diags: RefCell::new(Vec::new()),
            current: RefCell::new(String::new()),
            pending: RefCell::new(Vec::new()),
            ltype_vars: Cell::new(false),
            shareable: RefCell::new(Vec::new()),
            fin_spans: RefCell::new(Vec::new()),
            fin_span_default: Cell::new(Span::DUMMY),
            fin_held: RefCell::new(HashSet::new()),
        }
    }

    fn ev(&self) -> Ev<'_> {
        Ev::new(&self.prog, &self.metas, &self.fuel, &self.out_of_fuel)
    }

    fn report(&self, d: Diagnostic) {
        self.diags.borrow_mut().push(d);
    }

    fn error(&self, code: &str, span: Span, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::error(msg).with_code(code).with_label(span, "")
    }

    fn eval(&self, cx: &Cxt, t: &Term) -> Val {
        self.ev().eval(&cx.env, t)
    }

    fn quote(&self, cx: &Cxt, v: &Val) -> Term {
        self.ev().quote(cx.lvl(), v)
    }

    fn whnf(&self, v: &Val) -> Val {
        self.ev().whnf(v.clone())
    }

    fn ty_val(&self, cx: &Cxt, l: u32) -> Val {
        let env: Env = cx.env[..l as usize].to_vec();
        self.ev().eval(&env, &cx.tys[l as usize])
    }

    fn show(&self, cx: &Cxt, v: &Val) -> String {
        let t = self.quote(cx, v);
        print_term(&self.prog, &mut cx.names.clone(), &t)
    }

    fn fresh_meta(&self, cx: &Cxt, span: Span) -> Term {
        let m = self.metas.fresh();
        self.meta_spans.borrow_mut().push(span);
        self.meta_reported.borrow_mut().push(false);
        Term::InsertedMeta(m, Rc::new(cx.bound.clone()))
    }

    fn fresh_meta_masked(&self, mask: &[bool], span: Span) -> Term {
        let m = self.metas.fresh();
        self.meta_spans.borrow_mut().push(span);
        self.meta_reported.borrow_mut().push(false);
        Term::InsertedMeta(m, Rc::new(mask.to_vec()))
    }

    fn check_fuel(&self, span: Span) {
        if self.out_of_fuel.get() && self.fuel_span.get().is_none() {
            self.fuel_span.set(Some(span));
            self.report(
                self.error(
                    "E1220",
                    span,
                    "evaluation during type checking ran out of fuel",
                )
                .with_note(
                    "v0.0.1 does not check termination; a computation in a type does not finish",
                )
                .with_help("simplify the computation in the type"),
            );
        }
    }

    // ---- binders and usage ----

    #[allow(clippy::too_many_arguments)]
    fn push(
        &self,
        cx: &mut Cxt,
        name: &str,
        mult: Mult,
        ty: &Val,
        val: Option<Val>,
        span: Span,
        src: Option<&str>,
    ) -> u32 {
        let l = cx.lvl();
        cx.tys.push(self.ev().quote(l, ty));
        cx.bound.push(val.is_none());
        cx.env.push(val.unwrap_or(Val::var(l)));
        cx.names.push(name.into());
        cx.mults.push(mult);
        cx.spans.push(span);
        cx.uses.push(Usage::default());
        if let Some(s) = src {
            cx.scope.push((s.to_string(), l));
        }
        l
    }

    /// U-Bind: an ω binder must have a type in `Type`. A type that is not
    /// known yet is checked when the definition is finished.
    fn check_ubind(&self, cx: &Cxt, mult: Mult, ty: &Val, span: Span, what: &'static str) {
        if mult != Mult::Many {
            return;
        }
        if self.univ_of(cx, ty) == Univ::LType {
            self.report_ubind(cx, ty, span, what);
        } else if self.has_metas(cx, ty) {
            self.shareable
                .borrow_mut()
                .push((cx.clone(), ty.clone(), Val::Type, span, what));
        }
    }

    fn report_ubind(&self, cx: &Cxt, ty: &Val, span: Span, what: &str) {
        self.report(
            self.error(
                "E0301",
                span,
                format!(
                    "{what} of linear type `{}` is bound with multiplicity ω",
                    self.show(cx, ty)
                ),
            )
            .with_note("values of an LType may be used only once")
            .with_help(
                "bind it with multiplicity 1, e.g. `(1 x : A)`, or make it shareable with `freeze`",
            ),
        );
    }

    /// A value that may hold a linear value (its type does not show it) is
    /// shared (spec-v0.0.2.md §5.1): the linear value would be used once per
    /// use of the value.
    fn report_shared_holder(&self, at: Span, what: &str) {
        self.report(self.shared_holder(at, what).with_help(
            "bind the value with multiplicity 1, or create the linear value where it is used",
        ));
    }

    fn shared_holder(&self, at: Span, what: &str) -> Diagnostic {
        self.error(
            "E0305",
            at,
            "a linear value is captured by a value that may be used more than once",
        )
        .with_note(format!("the value that captures it is {what}"))
        .with_note("a closure or an action keeps the linear value it was built from")
    }

    /// Records that the linear value at `at` (of type `ty`), held by the
    /// value being built, has a finalizer.
    fn note_held(&self, cx: &Cxt, at: Span, ty: &Val) {
        if self.drop_status(cx, ty) == DropStatus::Fin {
            self.fin_held.borrow_mut().insert(at);
        }
    }

    /// A holder that is given up without being used drops the value it
    /// holds without its finalizer (spec-v0.0.2.md §5.5): a leak, which is
    /// safe but reported.
    fn warn_leak(&self, cx: &Cxt, l: u32, name: &str) {
        let Some(h) = cx.holds.get(&l) else {
            return;
        };
        if !self.fin_held.borrow().contains(h) {
            return;
        }
        self.report(
            Diagnostic::warning(format!(
                "`{name}` is never used, so the value it holds is not finalized"
            ))
            .with_code("W0308")
            .with_label(cx.spans[l as usize], "never used")
            .with_secondary(*h, "this value has a finalizer that will not run")
            .with_note("a closure or an action keeps the linear value it was built from")
            .with_help(format!(
                "use `{name}`, or create the linear value where it is used"
            )),
        );
    }

    /// The value of an expression of type `ty` may hold a linear value
    /// without showing it: its type is in `Type` and not resource-free
    /// (a function, an action or a data type that may contain one).
    fn may_hide_linear(&self, cx: &Cxt, ty: &Val) -> bool {
        self.univ_of(cx, ty) == Univ::Type && !self.resource_free(cx, ty)
    }

    fn has_metas(&self, cx: &Cxt, v: &Val) -> bool {
        fn go(t: &Term) -> bool {
            match t {
                Term::Meta(_) | Term::InsertedMeta(..) => true,
                Term::Pi(_, _, _, a, b) | Term::Lam(_, _, _, a, b) | Term::App(a, b, _) => {
                    go(a) || go(b)
                }
                Term::Let(_, _, a, v, b) => go(a) || go(v) || go(b),
                Term::Case(_) => true,
                _ => false,
            }
        }
        go(&self.quote(cx, v))
    }

    /// A type argument `t` of sort `sort` (`Type` or `… -> Type`) must be
    /// shareable (spec-v0.0.2.md §5.1); checked at once or, when it is not
    /// known yet, at the end of the definition.
    fn check_type_arg(&self, cx: &Cxt, t: &Val, sort: &Val, span: Span) {
        if !self.type_valued(cx.lvl(), sort) {
            return;
        }
        if self.has_metas(cx, t) {
            self.shareable.borrow_mut().push((
                cx.clone(),
                t.clone(),
                sort.clone(),
                span,
                "type argument",
            ));
        } else {
            self.check_shareable_one(cx, t, sort, span, "type argument");
        }
    }

    /// `Type` or a function sort ending in `Type`.
    fn type_valued(&self, lvl: u32, sort: &Val) -> bool {
        match self.whnf(sort) {
            Val::Type => true,
            Val::Pi(_, _, _, _, c) => self.type_valued(lvl + 1, &self.ev().inst(&c, Val::var(lvl))),
            _ => false,
        }
    }

    /// Runs the checks that waited for the metas of a definition.
    fn check_shareable(&self) {
        let items = std::mem::take(&mut *self.shareable.borrow_mut());
        for (cx, t, sort, span, what) in items {
            self.check_shareable_one(&cx, &t, &sort, span, what);
        }
    }

    /// Whether the type (constructor) `t` of sort `sort` is linear: a type
    /// constructor is applied to fresh shareable types.
    fn linear_arg(&self, cx: &Cxt, t: &Val, sort: &Val) -> bool {
        if matches!(self.whnf(t), Val::Error) {
            return false;
        }
        let mut cx2 = cx.clone();
        let mut app = t.clone();
        let mut s = self.whnf(sort);
        while let Val::Pi(x, _, i, dom, c) = s {
            let dom = if matches!(self.whnf(&dom), Val::LType) {
                Val::Type
            } else {
                (*dom).clone()
            };
            let l = self.push(&mut cx2, &x, Mult::Zero, &dom, None, Span::DUMMY, None);
            app = self.ev().apply(app, Val::var(l), i);
            s = self.whnf(&self.ev().inst(&c, Val::var(l)));
        }
        self.univ_of(&cx2, &app) == Univ::LType
    }

    fn check_shareable_one(&self, cx: &Cxt, t: &Val, sort: &Val, span: Span, what: &str) {
        if !self.linear_arg(cx, t, sort) {
            return;
        }
        if what == "type argument" {
            self.report(
                self.error(
                    "E0301",
                    span,
                    format!(
                        "a shareable type is expected here, but `{}` is linear",
                        self.show(cx, t)
                    ),
                )
                .with_note("this type parameter is declared in `Type`")
                .with_help("declare the parameter in `LType`, e.g. `{a : LType}`"),
            );
        } else {
            self.report_ubind(cx, t, span, what);
        }
    }

    /// Pops the last binder, checking its usage against its multiplicity.
    fn pop(&self, cx: &mut Cxt, check: bool) -> Usage {
        let l = cx.lvl() - 1;
        let u = cx.uses.pop().unwrap_or_default();
        if check && cx.mults[l as usize] == Mult::One {
            let name = cx.names[l as usize].clone();
            match u.mult() {
                Mult::Zero => {
                    let ty = self.ty_val(cx, l);
                    if self.can_weaken(cx, &ty) {
                        self.note_fin(cx, l, &ty);
                        self.warn_leak(cx, l, &name);
                    } else {
                        let d = self.error(
                            "E0302",
                            cx.spans[l as usize],
                            format!("linear value `{name}` is not consumed"),
                        );
                        self.report(self.why_not_weaken(
                            cx,
                            &ty,
                            d,
                            "consume it by pattern matching or by passing it to a function",
                        ));
                    }
                }
                // A value without resources may be copied (spec-v0.0.2.md
                // §5.1): it holds no linear resource.
                Mult::Many if self.resource_free(cx, &self.ty_val(cx, l)) => {}
                Mult::Many => {
                    let at = u
                        .spans
                        .get(1)
                        .or(u.spans.first())
                        .copied()
                        .unwrap_or(cx.spans[l as usize]);
                    let mut d = self.error(
                        "E0303",
                        at,
                        format!("linear variable `{name}` is used more than once"),
                    );
                    if let Some(first) = u.spans.first()
                        && u.spans.len() > 1
                    {
                        d = d.with_secondary(*first, "first use");
                    }
                    let help = if let Some(h) = cx.holds.get(&l) {
                        d = d
                            .with_secondary(*h, "its value holds this linear value")
                            .with_note(format!(
                                "`{name}` is linear because its value holds a linear value"
                            ));
                        "use it only once, or make it a function that creates the linear value at each call"
                    } else {
                        "use it only once, or make it shareable with `freeze`"
                    };
                    self.report(d.with_help(help));
                }
                Mult::One => {}
            }
        }
        cx.env.pop();
        cx.tys.pop();
        cx.bound.pop();
        cx.names.pop();
        cx.mults.pop();
        cx.spans.pop();
        cx.scope.retain(|(_, x)| *x < l);
        cx.shapes.remove(&l);
        cx.holds.remove(&l);
        u
    }

    fn use_var(&self, cx: &mut Cxt, l: u32, rig: Mult, span: Span) {
        if rig == Mult::Zero {
            return;
        }
        if cx.mults[l as usize] == Mult::Zero {
            self.report(
                self.error(
                    "E0304",
                    span,
                    format!(
                        "erased variable `{}` is used at runtime",
                        cx.names[l as usize]
                    ),
                )
                .with_note("variables of multiplicity 0 do not exist at runtime")
                .with_help("give the binder multiplicity ω (the default) or 1"),
            );
            return;
        }
        let u = &mut cx.uses[l as usize];
        u.m = Some(u.mult().plus(rig));
        if u.spans.len() < 2 {
            u.spans.push(span);
        }
    }

    fn enter_scope(&self, cx: &mut Cxt) -> Scope {
        let saved = cx.uses.clone();
        for u in &mut cx.uses {
            *u = Usage::default();
        }
        Scope {
            uses: saved,
            holders: std::mem::take(&mut cx.holders),
        }
    }

    /// Combines the usage of a scope (a lambda body, a let right-hand side)
    /// used `factor` times with the usage before it.
    fn exit_scope(&self, cx: &mut Cxt, saved: Scope, factor: Mult, code: &str, what: &str) {
        let held = std::mem::replace(&mut cx.holders, saved.holders);
        match factor {
            Mult::Many => {
                if let Some(at) = held.first() {
                    self.report_shared_holder(*at, what);
                }
            }
            Mult::One => cx.holders.extend(held),
            Mult::Zero => {}
        }
        let saved = saved.uses;
        let n = saved.len();
        for (l, s) in saved.into_iter().enumerate() {
            let cur = std::mem::take(&mut cx.uses[l]);
            let mut f = factor;
            if cur.mult() != Mult::Zero
                && factor == Mult::Many
                && cx.mults[l] == Mult::One
                && !self.resource_free(cx, &self.ty_val(cx, l as u32))
            {
                let at = cur.spans.first().copied().unwrap_or(cx.spans[l]);
                self.report(
                    self.error(
                        code,
                        at,
                        format!("linear variable `{}` is captured by {what}", cx.names[l]),
                    )
                    .with_note(format!(
                        "it may be evaluated more than once, so `{}` could be used more than once",
                        cx.names[l]
                    ))
                    .with_help("use the closure linearly, or pass the value as an argument"),
                );
                f = Mult::One;
            }
            let mut spans = s.spans.clone();
            if f != Mult::Zero {
                spans.extend(cur.spans.iter().copied());
                spans.truncate(2);
            }
            cx.uses[l] = Usage {
                m: Some(s.mult().plus(f.times(cur.mult()))),
                spans,
            };
        }
        cx.uses.truncate(n);
    }

    /// Joins the usages of the branches of a case.
    fn join(&self, cx: &mut Cxt, results: Vec<Vec<Usage>>, span: Span) {
        if results.is_empty() {
            return;
        }
        let n = cx.lvl() as usize;
        for l in 0..n {
            let ms: Vec<Mult> = results.iter().map(|r| r[l].mult()).collect();
            let all_eq = ms.iter().all(|m| *m == ms[0]);
            let max = if ms.contains(&Mult::Many) {
                Mult::Many
            } else if ms.contains(&Mult::One) {
                Mult::One
            } else {
                Mult::Zero
            };
            if !all_eq && cx.mults[l] == Mult::One {
                let ty = self.ty_val(cx, l as u32);
                if self.can_weaken(cx, &ty) {
                    self.note_fin(cx, l as u32, &ty);
                } else {
                    let d = self
                        .error(
                            "E0306",
                            span,
                            format!(
                                "linear variable `{}` is consumed in some branches but not in others",
                                cx.names[l]
                            ),
                        )
                        .with_secondary(cx.spans[l], "bound here");
                    self.report(self.why_not_weaken(cx, &ty, d, "consume it in every branch"));
                }
            }
            let mut spans = Vec::new();
            for r in &results {
                if r[l].mult() == max {
                    spans = r[l].spans.clone();
                    break;
                }
            }
            cx.uses[l] = Usage {
                m: Some(max),
                spans,
            };
        }
    }

    // ---- universes and Drop ----

    /// The universe of a type (spec-v0.0.2.md §5.1). Data types propagate
    /// the linearity of their `LType` parameters, and an applied type
    /// variable is linear when its sort ends in `LType` or when an argument
    /// in an `LType` position is linear (it may be such a data type).
    fn univ_of(&self, cx: &Cxt, ty: &Val) -> Univ {
        match self.whnf(ty) {
            Val::Data(d, args) => match self.prog.get(d) {
                Global::Data {
                    univ: Univ::LType, ..
                } => Univ::LType,
                Global::Data { lin_params, .. } => {
                    let lin = lin_params.iter().enumerate().any(|(k, lin)| {
                        *lin && args
                            .get(k)
                            .is_some_and(|(a, _)| self.univ_of(cx, a) == Univ::LType)
                    });
                    if lin { Univ::LType } else { Univ::Type }
                }
                _ => Univ::Type,
            },
            Val::Neutral(Head::Global(g), _) if Some(g) == self.prog.builtins.unique => Univ::LType,
            Val::Neutral(Head::Var(l), sp) if (l as usize) < cx.tys.len() => {
                let mut k = self.whnf(&self.ty_val(cx, l));
                for e in &sp {
                    let Elim::App(a, _) = e else { break };
                    let Val::Pi(_, _, _, dom, c) = k else { break };
                    if matches!(self.whnf(&dom), Val::LType) && self.univ_of(cx, a) == Univ::LType {
                        return Univ::LType;
                    }
                    k = self.whnf(&self.ev().inst(&c, a.clone()));
                }
                if matches!(k, Val::LType) {
                    Univ::LType
                } else {
                    Univ::Type
                }
            }
            _ => Univ::Type,
        }
    }

    /// Whether the data type `d` has a finalizer: an implementation of
    /// `Drop` or `Release` without a context. An implementation with a
    /// context (`impl Drop a => Drop (List a)`) only lifts the finalizers
    /// of the fields (spec-v0.0.2.md §5.3).
    fn has_fin_impl(&self, d: GName) -> bool {
        let b = &self.prog.builtins;
        [b.drop_trait, b.release_trait]
            .into_iter()
            .flatten()
            .any(|tr| {
                self.prog.impls.get(&tr).is_some_and(|is| {
                    is.iter().any(|i| {
                        i.nctx == 0
                            && matches!(self.impl_head_args(i).first().map(|a| self.whnf(&a.0)),
                            Some(Val::Data(h, _)) if h == d)
                    })
                })
            })
    }

    /// How a value of this type can be given up without being used
    /// (spec-v0.0.2.md §5.3).
    fn drop_status(&self, cx: &Cxt, ty: &Val) -> DropStatus {
        self.drop_status_go(cx, cx.lvl(), ty, &mut Vec::new())
    }

    fn drop_status_go(
        &self,
        cx: &Cxt,
        lvl: u32,
        ty: &Val,
        visiting: &mut Vec<GName>,
    ) -> DropStatus {
        let w = self.whnf(ty);
        if self.univ_of(cx, &w) == Univ::Type {
            return DropStatus::Free;
        }
        match w {
            Val::Data(d, args) => {
                if self.has_fin_impl(d) {
                    return DropStatus::Fin;
                }
                if visiting.contains(&d) {
                    return DropStatus::Free;
                }
                let Global::Data { cons, nparams, .. } = self.prog.get(d).clone() else {
                    return DropStatus::Free;
                };
                if cons.is_empty() {
                    return DropStatus::Stuck;
                }
                visiting.push(d);
                let mut st = DropStatus::Free;
                for c in cons {
                    let Some(cty) = self.prog.type_of(c) else {
                        continue;
                    };
                    let mut v = self.ev().eval(&Vec::new(), cty);
                    for (a, _) in args.iter().take(nparams) {
                        match self.whnf(&v) {
                            Val::Pi(_, _, _, _, clo) => v = self.ev().inst(&clo, a.clone()),
                            _ => break,
                        }
                    }
                    let mut l = lvl;
                    while let Val::Pi(_, m, _, a, clo) = self.whnf(&v) {
                        if m != Mult::Zero {
                            st = st.max(self.drop_status_go(cx, l, &a, visiting));
                        }
                        v = self.ev().inst(&clo, Val::var(l));
                        l += 1;
                    }
                }
                visiting.pop();
                st
            }
            // A linear type variable: given up through `Drop` / `Release`
            // from the context.
            Val::Neutral(Head::Var(_), _) => DropStatus::Fin,
            _ => DropStatus::Free,
        }
    }

    /// The note and help of E0302 / E0306.
    fn why_not_weaken(&self, cx: &Cxt, ty: &Val, d: Diagnostic, help: &str) -> Diagnostic {
        let t = self.show(cx, ty);
        if self.drop_status(cx, ty) == DropStatus::Fin {
            d.with_note(format!(
                "`{t}` holds values with a finalizer, but has no `Drop` or `Release` implementation"
            ))
            .with_help("derive one, e.g. `deriving (Release)`, or consume the value on every path")
        } else {
            d.with_note(format!("its type `{t}` cannot be dropped implicitly"))
                .with_help(help)
        }
    }

    /// Records a linear binder that some path leaves unused, when it may
    /// need a finalizer (decided by the RAII pass on the solved program).
    fn note_fin(&self, cx: &Cxt, l: u32, ty: &Val) {
        if self.drop_status(cx, ty) == DropStatus::Fin || self.has_metas(cx, ty) {
            self.fin_spans
                .borrow_mut()
                .push((cx.names[l as usize].clone(), cx.spans[l as usize]));
        }
    }

    /// Weaken-Drop / Weaken-Release: a linear value of this type may be
    /// left unused (the finalizer is inserted by the RAII pass).
    fn can_weaken(&self, cx: &Cxt, ty: &Val) -> bool {
        match self.drop_status(cx, ty) {
            DropStatus::Free => true,
            DropStatus::Stuck => false,
            DropStatus::Fin => {
                let b = &self.prog.builtins;
                // An answer that depends on unsolved metas is left to the
                // RAII pass, which runs on the solved program.
                [b.drop_trait, b.release_trait]
                    .into_iter()
                    .flatten()
                    .any(|tr| {
                        let goal = Val::Data(tr, vec![(ty.clone(), Icit::Expl)]);
                        !matches!(self.search(cx, &goal, 0), Search::NotFound)
                    })
            }
        }
    }

    /// Values of this type cannot contain linear resources (spec §4.4):
    /// primitive scalars and strings, and `Type` data built only from them.
    fn resource_free(&self, cx: &Cxt, ty: &Val) -> bool {
        self.resource_free_go(cx.lvl(), ty, &mut Vec::new(), &mut false)
    }

    /// `Some(resource_free)`, or `None` when the answer depends on an
    /// unsolved meta (e.g. `List ?a`).
    fn resource_free_known(&self, cx: &Cxt, ty: &Val) -> Option<bool> {
        let mut blocked = false;
        let rf = self.resource_free_go(cx.lvl(), ty, &mut Vec::new(), &mut blocked);
        if rf || !blocked { Some(rf) } else { None }
    }

    fn resource_free_go(
        &self,
        lvl: u32,
        ty: &Val,
        visiting: &mut Vec<GName>,
        blocked: &mut bool,
    ) -> bool {
        match self.whnf(ty) {
            Val::Neutral(Head::Meta(_), _) => {
                *blocked = true;
                false
            }
            Val::Neutral(Head::Global(g), sp) if sp.is_empty() => matches!(
                self.prog.prim_of(g),
                Some(Prim::Int | Prim::UInt | Prim::Float | Prim::Char | Prim::String)
            ),
            Val::Data(d, args) => {
                if visiting.contains(&d) {
                    return true;
                }
                let Global::Data {
                    univ: Univ::Type,
                    cons,
                    nparams,
                    ..
                } = self.prog.get(d).clone()
                else {
                    return false;
                };
                visiting.push(d);
                let mut ok = true;
                'cons: for c in cons {
                    let Some(cty) = self.prog.type_of(c) else {
                        continue;
                    };
                    let mut v = self.ev().eval(&Vec::new(), cty);
                    for (a, _) in args.iter().take(nparams) {
                        match self.whnf(&v) {
                            Val::Pi(_, _, _, _, clo) => v = self.ev().inst(&clo, a.clone()),
                            _ => break,
                        }
                    }
                    let mut l = lvl;
                    while let Val::Pi(_, m, _, a, clo) = self.whnf(&v) {
                        if m != Mult::Zero && !self.resource_free_go(l, &a, visiting, blocked) {
                            ok = false;
                            break 'cons;
                        }
                        v = self.ev().inst(&clo, Val::var(l));
                        l += 1;
                    }
                }
                visiting.pop();
                ok
            }
            _ => false,
        }
    }

    fn is_universe_valued(&self, lvl: u32, ty: &Val) -> bool {
        match self.whnf(ty) {
            Val::Type | Val::LType => true,
            Val::Pi(_, _, _, _, c) => {
                self.is_universe_valued(lvl + 1, &self.ev().inst(&c, Val::var(lvl)))
            }
            _ => false,
        }
    }

    // ---- unification with diagnostics ----

    fn unify_or_err(&self, cx: &Cxt, span: Span, expected: &Val, actual: &Val) -> bool {
        let ev = self.ev();
        match ev.sub(cx.lvl(), expected, actual, self.ltype_vars.get()) {
            Ok(()) => {
                self.check_fuel(span);
                true
            }
            Err(_) => {
                self.check_fuel(span);
                if self.out_of_fuel.get() {
                    return false;
                }
                // A type may depend on a constraint that has not been solved.
                if self.solve_pending(false)
                    && ev
                        .sub(cx.lvl(), expected, actual, self.ltype_vars.get())
                        .is_ok()
                {
                    return true;
                }
                // The types are shown with the literals at their defaults.
                self.default_literals();
                self.report(
                    self.error("E1200", span, "type mismatch")
                        .with_note(format!("expected `{}`", self.show(cx, expected)))
                        .with_note(format!("   found `{}`", self.show(cx, actual))),
                );
                false
            }
        }
    }

    fn builtin(&self, g: Option<GName>, what: &str, span: Span) -> Option<GName> {
        if g.is_none() {
            self.report(
                self.error("E1101", span, format!("`{what}` is not available"))
                    .with_note("this syntax needs the Prelude")
                    .with_help("import the Prelude"),
            );
        }
        g
    }

    // ---- checking ----

    fn check_type(&self, cx: &mut Cxt, e: &rast::Expr) -> Term {
        if matches!(e.kind, ExprKind::Unit | ExprKind::Tuple(_)) {
            return self.check(cx, e, &Val::LType, Mult::Zero);
        }
        let (t, s) = self.infer(cx, e, Mult::Zero);
        match self.whnf(&s) {
            Val::Type | Val::LType | Val::Error => t,
            Val::Neutral(Head::Meta(_), _) => {
                self.unify_or_err(cx, sp(e), &Val::Type, &s);
                t
            }
            other => {
                self.report(
                    self.error("E1200", sp(e), "expected a type")
                        .with_note(format!(
                            "this expression has type `{}`",
                            self.show(cx, &other)
                        )),
                );
                Term::Error
            }
        }
    }

    fn check(&self, cx: &mut Cxt, e: &rast::Expr, ty: &Val, rig: Mult) -> Term {
        let span = sp(e);
        let wty = self.whnf(ty);
        if let Val::Pi(x, m, Icit::Impl, a, c) = &wty {
            // A runtime lambda (a dictionary) evaluates its body at each call.
            let held = (*m != Mult::Zero).then(|| std::mem::take(&mut cx.holders));
            let l = self.push(cx, x, *m, a, None, span, None);
            let b = self.ev().inst(c, Val::var(l));
            let body = self.check(cx, e, &b, rig);
            self.pop(cx, false);
            if let Some(h) = held {
                cx.holders = h;
            }
            let at = self.quote(cx, a);
            return Term::Lam(x.clone(), *m, Icit::Impl, Rc::new(at), Rc::new(body));
        }
        match &e.kind {
            ExprKind::Lam(bs, body) if matches!(wty, Val::Pi(..)) => {
                self.check_lam(cx, bs, body, &wty, rig, span)
            }
            ExprKind::Let(bs, body) => self.elab_let(cx, bs, body, Some(ty), rig).0,
            ExprKind::Case(s, alts) => self.elab_case(cx, s, alts, ty, rig, span),
            ExprKind::If(c, t, f) => match self.desugar_if(c, t, f, span) {
                Some(d) => self.check(cx, &d, ty, rig),
                None => Term::Error,
            },
            ExprKind::Do(ss) => {
                if self.io_kind(&wty).is_some() {
                    return self.elab_io_do(cx, ss, &wty, rig, span);
                }
                match self.desugar_do(ss, span, false) {
                    Some(d) => self.check(cx, &d, ty, rig),
                    None => Term::Error,
                }
            }
            ExprKind::Lit(SLit::Int(n)) => self.check_int(cx, *n, false, &wty, span),
            ExprKind::Lit(SLit::Float(x)) if !matches!(wty, Val::Neutral(Head::Meta(_), _)) => {
                if self.is_prim_type(&wty, self.prog.builtins.float) {
                    Term::Lit(Lit::Float(*x))
                } else {
                    self.fallback(cx, e, ty, rig)
                }
            }
            ExprKind::App(f, a)
                if matches!(f.kind, ExprKind::Global(g) if Some(g) == self.prog.builtins.negate)
                    && matches!(
                        a.kind,
                        ExprKind::Lit(SLit::Int(_)) | ExprKind::Lit(SLit::Float(_))
                    ) =>
            {
                match &a.kind {
                    ExprKind::Lit(SLit::Int(n)) => self.check_int(cx, *n, true, &wty, span),
                    ExprKind::Lit(SLit::Float(x))
                        if self.is_prim_type(&wty, self.prog.builtins.float) =>
                    {
                        Term::Lit(Lit::Float(-*x))
                    }
                    _ => self.fallback(cx, e, ty, rig),
                }
            }
            ExprKind::Unit if matches!(wty, Val::Type | Val::LType) => {
                match self.builtin(self.prog.builtins.unit, "Unit", span) {
                    Some(g) => Term::Global(g),
                    None => Term::Error,
                }
            }
            ExprKind::Tuple(es) if matches!(wty, Val::Type | Val::LType) => {
                let Some(pair) = self.builtin(self.prog.builtins.pair, "Pair", span) else {
                    return Term::Error;
                };
                // The components may be linear (`Pair` propagates it).
                let ts: Vec<Term> = es
                    .iter()
                    .map(|x| self.check(cx, x, &Val::LType, Mult::Zero))
                    .collect();
                let mut it = ts.into_iter().rev();
                let mut acc = it.next().unwrap_or(Term::Error);
                for t in it {
                    acc = Term::App(
                        Rc::new(Term::App(
                            Rc::new(Term::Global(pair)),
                            Rc::new(t),
                            Icit::Expl,
                        )),
                        Rc::new(acc),
                        Icit::Expl,
                    );
                }
                if matches!(wty, Val::Type) {
                    let v = self.eval(cx, &acc);
                    self.check_type_arg(cx, &v, &wty, span);
                }
                acc
            }
            ExprKind::Tuple(_) | ExprKind::List(_) => match self.desugar_value(e) {
                Some(d) => self.check(cx, &d, ty, rig),
                None => Term::Error,
            },
            ExprKind::Hole(h) => {
                let mut d = self
                    .error("E1211", span, format!("hole `?{h}`"))
                    .with_note(format!("goal: {}", self.show(cx, ty)));
                for l in 0..cx.lvl() {
                    let n = &cx.names[l as usize];
                    if n.contains('$') {
                        continue;
                    }
                    let t = self.ty_val(cx, l);
                    let mut sub = cx.clone();
                    sub.env.truncate(l as usize);
                    sub.names.truncate(l as usize);
                    d = d.with_note(format!(
                        "  {} {n} : {}",
                        cx.mults[l as usize].name(),
                        self.show(&sub, &t)
                    ));
                }
                self.report(d);
                Term::Error
            }
            ExprKind::Wildcard => self.fresh_meta(cx, span),
            _ => self.fallback(cx, e, ty, rig),
        }
    }

    fn fallback(&self, cx: &mut Cxt, e: &rast::Expr, ty: &Val, rig: Mult) -> Term {
        let (t, a) = self.infer(cx, e, rig);
        let (t, a) = self.insert_implicits(cx, t, a, sp(e));
        if self.unify_or_err(cx, sp(e), ty, &a) {
            if self.type_valued(cx.lvl(), ty) {
                let tv = self.eval(cx, &t);
                self.check_type_arg(cx, &tv, ty, sp(e));
            }
            t
        } else {
            Term::Error
        }
    }

    fn is_prim_type(&self, v: &Val, g: Option<GName>) -> bool {
        matches!(v, Val::Neutral(Head::Global(h), sp) if sp.is_empty() && Some(*h) == g)
    }

    fn check_int(&self, cx: &mut Cxt, n: u128, neg: bool, wty: &Val, span: Span) -> Term {
        let b = &self.prog.builtins;
        let too_big = |what: &str| {
            self.report(self.error("E1200", span, format!("literal out of range for `{what}`")));
            Term::Error
        };
        if matches!(wty, Val::Neutral(Head::Meta(_), _)) {
            // Decided when the type is known (`1 + x`), `Int` by default.
            let meta = self.fresh_meta(cx, span);
            self.pending.borrow_mut().push(Pending {
                meta: meta.clone(),
                cx: cx.clone(),
                goal: Goal::Int {
                    n,
                    neg,
                    ty: wty.clone(),
                },
                span,
            });
            return meta;
        }
        if self.is_prim_type(wty, b.int) {
            let v = if neg { -(n as i128) } else { n as i128 };
            if v < i64::MIN as i128 || v > i64::MAX as i128 {
                return too_big("Int");
            }
            return Term::Lit(Lit::Int(v as i64));
        }
        if self.is_prim_type(wty, b.uint) && !neg {
            if n > u64::MAX as u128 {
                return too_big("UInt");
            }
            return Term::Lit(Lit::UInt(n as u64));
        }
        if self.is_prim_type(wty, b.float) {
            let x = n as f64;
            return Term::Lit(Lit::Float(if neg { -x } else { x }));
        }
        if let Val::Data(d, _) = wty
            && Some(*d) == b.nat
            && !neg
            && let (Some(z), Some(s)) = (b.zero, b.succ)
        {
            if n > 10_000 {
                return too_big("Nat (at most 10000)");
            }
            let mut t = Term::Global(z);
            for _ in 0..n {
                t = Term::App(Rc::new(Term::Global(s)), Rc::new(t), Icit::Expl);
            }
            return t;
        }
        // Any other type: `fromInt`, if it has a `Num` instance.
        let v = if neg { -(n as i128) } else { n as i128 };
        if let (Some(num), Some(from_int)) = (b.num, b.from_int)
            && v >= i64::MIN as i128
            && v <= i64::MAX as i128
        {
            let goal = Val::Data(num, vec![(wty.clone(), Icit::Expl)]);
            let dict = match self.search(cx, &goal, 0) {
                Search::Found(d) => Some(d),
                Search::NotFound => None,
                Search::Stuck => {
                    let meta = self.fresh_meta(cx, span);
                    self.pending.borrow_mut().push(Pending {
                        meta: meta.clone(),
                        cx: cx.clone(),
                        goal: Goal::Instance(goal),
                        span,
                    });
                    Some(meta)
                }
            };
            if let Some(d) = dict {
                let t = Term::App(
                    Rc::new(Term::Global(from_int)),
                    Rc::new(self.quote(cx, wty)),
                    Icit::Impl,
                );
                let t = Term::App(Rc::new(t), Rc::new(d), Icit::Impl);
                return Term::App(
                    Rc::new(t),
                    Rc::new(Term::Lit(Lit::Int(v as i64))),
                    Icit::Expl,
                );
            }
        }
        // Infer as Int and report the mismatch.
        let it = match b.int {
            Some(g) => Val::Neutral(Head::Global(g), Vec::new()),
            None => Val::Error,
        };
        self.unify_or_err(cx, span, wty, &it);
        Term::Lit(Lit::Int(v as i64))
    }

    fn check_lam(
        &self,
        cx: &mut Cxt,
        bs: &[rast::LamBinder],
        body: &rast::Expr,
        ty: &Val,
        rig: Mult,
        span: Span,
    ) -> Term {
        let saved = self.enter_scope(cx);
        let inner = if rig == Mult::Zero {
            Mult::Zero
        } else {
            Mult::One
        };
        let t = self.check_lam_go(cx, bs, body, ty, inner, span);
        // The body of a runtime lambda is evaluated at each call, so the
        // linear values it creates are not shared.
        if runtime_lam(&t) {
            cx.holders.clear();
        }
        self.exit_scope(cx, saved, rig, "E0305", "a closure used many times");
        t
    }

    fn check_lam_go(
        &self,
        cx: &mut Cxt,
        bs: &[rast::LamBinder],
        body: &rast::Expr,
        ty: &Val,
        rig: Mult,
        span: Span,
    ) -> Term {
        let Some((b, rest)) = bs.split_first() else {
            return self.check(cx, body, ty, rig);
        };
        let wty = self.whnf(ty);
        let Val::Pi(px, m, Icit::Expl, a, c) = wty else {
            if let Val::Pi(_, _, Icit::Impl, _, _) = wty {
                // Insert the implicit lambda, then continue.
                let e = mk(ExprKind::Lam(bs.to_vec(), Box::new(body.clone())), span);
                return self.check(cx, &e, ty, rig);
            }
            self.report(
                self.error(
                    "E1200",
                    b.span,
                    "this lambda is not expected to be a function",
                )
                .with_note(format!("expected `{}`", self.show(cx, ty))),
            );
            return Term::Error;
        };
        if let Some(bm) = b.mult {
            let bm = conv_mult(bm);
            if bm != m {
                self.report(self.error(
                    "E1200",
                    b.span,
                    format!(
                        "multiplicity mismatch: the binder has {} but the type expects {}",
                        bm.name(),
                        m.name()
                    ),
                ));
            }
        }
        if let Some(t) = &b.ty {
            let tt = self.check_type(cx, t);
            let tv = self.eval(cx, &tt);
            self.unify_or_err(cx, t.span, &a, &tv);
        }
        let name: Name = b.name.clone().map(Into::into).unwrap_or(px.clone());
        if rig != Mult::Zero {
            self.check_ubind(cx, m, &a, b.span, "parameter");
        }
        let l = self.push(cx, &name, m, &a, None, b.span, b.name.as_deref());
        let cod = self.ev().inst(&c, Val::var(l));
        let bt = self.check_lam_go(cx, rest, body, &cod, rig, span);
        self.pop(cx, rig != Mult::Zero);
        let at = self.quote(cx, &a);
        Term::Lam(name, m, Icit::Expl, Rc::new(at), Rc::new(bt))
    }

    // ---- inference ----

    fn insert_implicits(&self, cx: &Cxt, mut t: Term, mut a: Val, span: Span) -> (Term, Val) {
        loop {
            match self.whnf(&a) {
                Val::Pi(_, _, Icit::Impl, dom, c) => {
                    let m = self.fresh_meta(cx, span);
                    let mv = self.eval(cx, &m);
                    self.check_type_arg(cx, &mv, &dom, span);
                    if self.trait_of(&dom).is_some() {
                        self.pending.borrow_mut().push(Pending {
                            meta: m.clone(),
                            cx: cx.clone(),
                            goal: Goal::Instance((*dom).clone()),
                            span,
                        });
                    }
                    t = Term::App(Rc::new(t), Rc::new(m), Icit::Impl);
                    a = self.ev().inst(&c, mv);
                }
                _ => return (t, a),
            }
        }
    }

    fn global_type(&self, g: GName) -> Option<Val> {
        self.prog.type_of(g).map(|t| self.ev().eval(&Vec::new(), t))
    }

    fn infer(&self, cx: &mut Cxt, e: &rast::Expr, rig: Mult) -> (Term, Val) {
        let span = sp(e);
        match &e.kind {
            ExprKind::Local(x) => match cx.lookup(x) {
                Some(l) => {
                    self.use_var(cx, l, rig, span);
                    (Term::Var(cx.ix(l)), self.ty_val(cx, l))
                }
                None => {
                    self.report(
                        Diagnostic::ice(format!("unbound local `{x}` after resolution"))
                            .with_label(span, ""),
                    );
                    (Term::Error, Val::Error)
                }
            },
            ExprKind::Global(g) => match self.global_type(*g) {
                Some(t) => (Term::Global(*g), t),
                None => (Term::Error, Val::Error),
            },
            ExprKind::Lit(l) => {
                let b = &self.prog.builtins;
                let (lit, ty) = match l {
                    SLit::Int(n) => {
                        if *n > i64::MAX as u128 {
                            self.report(self.error(
                                "E1200",
                                span,
                                "literal out of range for `Int`",
                            ));
                        }
                        (Lit::Int(*n as i64), b.int)
                    }
                    SLit::Float(x) => (Lit::Float(*x), b.float),
                    SLit::Char(c) => (Lit::Char(*c), b.char_),
                    SLit::Str(s) => (Lit::Str(s.as_str().into()), b.string),
                };
                let ty = match ty {
                    Some(g) => Val::Neutral(Head::Global(g), Vec::new()),
                    None => {
                        self.report(self.error("E1101", span, "primitive literal types are not available (the Prelude is not imported)"));
                        Val::Error
                    }
                };
                (Term::Lit(lit), ty)
            }
            ExprKind::Type | ExprKind::LType => (
                if matches!(e.kind, ExprKind::Type) {
                    Term::Type
                } else {
                    Term::LType
                },
                Val::Type,
            ),
            ExprKind::Unit => match (self.prog.builtins.mk_unit, self.prog.builtins.unit) {
                (Some(mk), Some(u)) => (Term::Global(mk), Val::Data(u, Vec::new())),
                _ => {
                    self.builtin(None, "Unit", span);
                    (Term::Error, Val::Error)
                }
            },
            ExprKind::App(f, a) => {
                if let ExprKind::Global(g) = f.kind
                    && Some(g) == self.prog.builtins.negate
                    && let ExprKind::Lit(SLit::Int(n)) = a.kind
                    && let Some(int) = self.prog.builtins.int
                {
                    return (
                        Term::Lit(Lit::Int((n as i128).wrapping_neg() as i64)),
                        Val::Neutral(Head::Global(int), Vec::new()),
                    );
                }
                self.infer_app(cx, e, rig)
            }
            ExprKind::Lam(bs, body) => self.infer_lam(cx, bs, body, rig, span),
            ExprKind::Let(bs, body) => self.elab_let(cx, bs, body, None, rig),
            ExprKind::Case(..) | ExprKind::If(..) | ExprKind::Do(..) | ExprKind::Hole(_) => {
                // An inferred match has a non-dependent type: it does not
                // depend on the matched variable, which the branches refine.
                let mut mask = cx.bound.clone();
                if let ExprKind::Case(s, _) | ExprKind::If(s, _, _) = &e.kind
                    && let ExprKind::Local(x) = &s.kind
                    && let Some(l) = cx.lookup(x)
                {
                    mask[l as usize] = false;
                }
                let m = self.fresh_meta_masked(&mask, span);
                let ty = self.eval(cx, &m);
                let t = self.check(cx, e, &ty, rig);
                (t, ty)
            }
            ExprKind::Wildcard => {
                let m = self.fresh_meta(cx, span);
                let ty = self.eval(cx, &m);
                (self.fresh_meta(cx, span), ty)
            }
            ExprKind::Pi {
                mult,
                implicit,
                name,
                name_span,
                dom,
                cod,
            } => {
                let a = self.check_type(cx, dom);
                let av = self.eval(cx, &a);
                if *implicit
                    && name.is_none()
                    && self.trait_of(&av).is_none()
                    && !matches!(self.whnf(&av), Val::Error)
                {
                    self.report(
                        self.error("E1250", dom.span, "a constraint must be a trait")
                            .with_note(format!("this is `{}`", self.show(cx, &av))),
                    );
                }
                let univ = self.is_universe_valued(cx.lvl(), &av);
                let m = match mult {
                    Some(m) => {
                        let m = conv_mult(*m);
                        if m != Mult::Zero && univ {
                            self.report(
                                self.error("E1240", *name_span, "a type is passed at runtime")
                                    .with_note("types do not exist at runtime")
                                    .with_help("give the binder multiplicity 0 (or leave it out)"),
                            );
                        }
                        m
                    }
                    None if *implicit || univ => Mult::Zero,
                    None => Mult::Many,
                };
                let x: Name = name.clone().unwrap_or_else(|| "_".into()).into();
                self.push(cx, &x, m, &av, None, *name_span, name.as_deref());
                let b = self.check_type(cx, cod);
                self.pop(cx, false);
                let i = if *implicit { Icit::Impl } else { Icit::Expl };
                (Term::Pi(x, m, i, Rc::new(a), Rc::new(b)), Val::Type)
            }
            ExprKind::Ann(x, t) => {
                let tt = self.check_type(cx, t);
                let tv = self.eval(cx, &tt);
                let xt = self.check(cx, x, &tv, rig);
                (xt, tv)
            }
            ExprKind::Tuple(es) => {
                // A tuple of types is a type.
                if let Some(first) = es.first() {
                    let mut probe = cx.clone();
                    let before = self.diags.borrow().len();
                    let (_, s) = self.infer(&mut probe, first, Mult::Zero);
                    self.diags.borrow_mut().truncate(before);
                    if matches!(self.whnf(&s), Val::Type | Val::LType) {
                        return (self.check(cx, e, &Val::LType, Mult::Zero), Val::Type);
                    }
                }
                match self.desugar_value(e) {
                    Some(d) => self.infer(cx, &d, rig),
                    None => (Term::Error, Val::Error),
                }
            }
            ExprKind::List(_) => match self.desugar_value(e) {
                Some(d) => self.infer(cx, &d, rig),
                None => (Term::Error, Val::Error),
            },
        }
    }

    fn infer_app(&self, cx: &mut Cxt, e: &rast::Expr, rig: Mult) -> (Term, Val) {
        let mut args = Vec::new();
        let mut h = e;
        while let ExprKind::App(f, a) = &h.kind {
            args.push(a.as_ref());
            h = f;
        }
        args.reverse();
        let (mut t, mut ty) = self.infer(cx, h, rig);
        // Constructor fields are used as often as the constructed value
        // (`1·ω = 1`, spec §4.5).
        let is_con =
            matches!(h.kind, ExprKind::Global(g) if matches!(self.prog.get(g), Global::Con { .. }));
        let mut span = sp(h);
        // A linear argument that is not a variable (whose usage is tracked).
        let mut lin_arg = None;
        for a in args {
            let (t2, ty2) = self.insert_implicits(cx, t, ty, span);
            t = t2;
            ty = ty2;
            // A value of unknown type that is applied is a function:
            // refine `?m` to `(x : ?A) -> ?B x`.
            if let Val::Neutral(Head::Meta(_), sp_) = self.whnf(&ty) {
                // The new metas may only depend on the variables the
                // applied meta can see (its spine), or solving it escapes.
                let mut mask = vec![false; cx.bound.len()];
                for e in &sp_ {
                    if let Elim::App(v, _) = e
                        && let Val::Neutral(Head::Var(l), es) = self.whnf(v)
                        && es.is_empty()
                        && (l as usize) < mask.len()
                    {
                        mask[l as usize] = cx.bound[l as usize];
                    }
                }
                let dom = self.fresh_meta_masked(&mask, span);
                let dv = self.eval(cx, &dom);
                let mut cx2 = cx.clone();
                self.push(&mut cx2, "x", Mult::Many, &dv, None, span, None);
                // Non-dependent: the codomain does not mention `x`.
                mask.push(false);
                let cod = self.fresh_meta_masked(&mask, span);
                let pi = Term::Pi(
                    "x".into(),
                    Mult::Many,
                    Icit::Expl,
                    Rc::new(dom),
                    Rc::new(cod),
                );
                let pv = self.eval(cx, &pi);
                if self.ev().unify(cx.lvl(), &ty, &pv).is_ok() {
                    ty = pv;
                }
            }
            match self.whnf(&ty) {
                Val::Pi(_, m, Icit::Expl, dom, c) => {
                    let known = if m == Mult::Many && !is_con && rig != Mult::Zero {
                        self.resource_free_known(cx, &dom)
                    } else {
                        Some(false)
                    };
                    let at = if known.is_none() {
                        // Whether the argument is resource-free is only known
                        // after checking it (`length xs` with `List ?a`):
                        // check it as a scope and scale its usage afterwards.
                        let saved = self.enter_scope(cx);
                        let at = self.check(cx, a, &dom, Mult::One);
                        let f = if self.resource_free(cx, &dom) {
                            Mult::One
                        } else {
                            Mult::Many
                        };
                        let (code, what) = if matches!(a.kind, ExprKind::Lam(..)) {
                            ("E0305", "a closure used many times")
                        } else {
                            ("E0303", "an argument used many times")
                        };
                        self.exit_scope(cx, saved, rig.times(f), code, what);
                        at
                    } else {
                        let m = if m == Mult::Many && (is_con || known == Some(true)) {
                            Mult::One
                        } else {
                            m
                        };
                        self.check(cx, a, &dom, rig.times(m))
                    };
                    if m != Mult::Zero
                        && lin_arg.is_none()
                        && !matches!(a.kind, ExprKind::Local(_))
                        && self.univ_of(cx, &dom) == Univ::LType
                    {
                        lin_arg = Some(sp(a));
                        self.note_held(cx, sp(a), &dom);
                    }
                    let av = self.eval(cx, &at);
                    t = Term::App(Rc::new(t), Rc::new(at), Icit::Expl);
                    ty = self.ev().inst(&c, av);
                }
                Val::Error => {
                    self.check(cx, a, &Val::Error, rig);
                    t = Term::Error;
                    ty = Val::Error;
                }
                other => {
                    self.report(
                        self.error(
                            "E1204",
                            sp(a),
                            "too many arguments, or a value that is not a function is applied",
                        )
                        .with_secondary(span, format!("this has type `{}`", self.show(cx, &other))),
                    );
                    return (Term::Error, Val::Error);
                }
            }
            span = span.to(sp(a));
        }
        // The result may keep the linear argument (a partial application, an
        // action or a closure in a data value) without showing it in its
        // type (spec-v0.0.2.md §5.1).
        if let Some(at) = lin_arg
            && rig != Mult::Zero
            && self.may_hide_linear(cx, &ty)
        {
            if rig == Mult::Many {
                self.report_shared_holder(at, "used many times");
            } else {
                cx.holders.push(at);
            }
        }
        (t, ty)
    }

    fn infer_lam(
        &self,
        cx: &mut Cxt,
        bs: &[rast::LamBinder],
        body: &rast::Expr,
        rig: Mult,
        span: Span,
    ) -> (Term, Val) {
        // Build the expected type from annotations (or metas), then check.
        let mut pis = Vec::new();
        let mut sub = cx.clone();
        for b in bs {
            let a = match &b.ty {
                Some(t) => self.check_type(&mut sub, t),
                None => self.fresh_meta(&sub, b.span),
            };
            let av = self.eval(&sub, &a);
            let m = match b.mult {
                Some(m) => conv_mult(m),
                None if self.is_universe_valued(sub.lvl(), &av) => Mult::Zero,
                None => Mult::Many,
            };
            let x: Name = b.name.clone().unwrap_or_else(|| "_".into()).into();
            self.push(&mut sub, &x, m, &av, None, b.span, b.name.as_deref());
            pis.push((x, m, a));
        }
        let r = self.fresh_meta(&sub, span);
        let mut ty = r;
        for (x, m, a) in pis.into_iter().rev() {
            ty = Term::Pi(x, m, Icit::Expl, Rc::new(a), Rc::new(ty));
        }
        let tv = self.eval(cx, &ty);
        let t = self.check(
            cx,
            &mk(ExprKind::Lam(bs.to_vec(), Box::new(body.clone())), span),
            &tv,
            rig,
        );
        (t, tv)
    }

    fn elab_let(
        &self,
        cx: &mut Cxt,
        bs: &[rast::LetBind],
        body: &rast::Expr,
        expected: Option<&Val>,
        rig: Mult,
    ) -> (Term, Val) {
        // The whole `let` is checked as a scope used `rig` times, so the
        // usage of its own binders is counted relative to one evaluation of
        // the `let` (a `let 1` under an ω argument is still fine).
        let outer_saved = self.enter_scope(cx);
        let outer_rig = rig;
        let rig = if rig == Mult::Zero {
            Mult::Zero
        } else {
            Mult::One
        };
        let mut binds = Vec::new();
        // The first linear binder that the result may hold.
        let mut lin_bind = None;
        for b in bs {
            let saved = self.enter_scope(cx);
            let inner = rig;
            let (vt, ty) = match &b.ty {
                Some(t) => {
                    let tt = self.check_type(cx, t);
                    let tv = self.eval(cx, &tt);
                    (self.check(cx, &b.rhs, &tv, inner), tv)
                }
                None => {
                    let (t, a) = self.infer(cx, &b.rhs, inner);
                    self.insert_implicits(cx, t, a, b.rhs.span)
                }
            };
            let uses_linear = cx.uses.iter().enumerate().any(|(l, u)| {
                u.mult() != Mult::Zero
                    && cx.mults[l] == Mult::One
                    && !self.resource_free(cx, &self.ty_val(cx, l as u32))
            });
            // The value may hold a linear value it was built from.
            let holds = !cx.holders.is_empty();
            // Prefer a held value with a finalizer (for W0308).
            let held_at = {
                let fh = self.fin_held.borrow();
                cx.holders
                    .iter()
                    .find(|h| fh.contains(h))
                    .or(cx.holders.first())
                    .copied()
            };
            let rf = self.resource_free(cx, &ty);
            let m = match b.mult {
                Some(m) => conv_mult(m),
                None if self.univ_of(cx, &ty) == Univ::LType || ((uses_linear || holds) && !rf) => {
                    Mult::One
                }
                None => Mult::Many,
            };
            let factor = if m == Mult::Many && rf { Mult::One } else { m };
            // A linear binder holds them (its uses are tracked), and a
            // resource-free value cannot.
            if m != Mult::Many || rf {
                cx.holders.clear();
            }
            if m == Mult::One && !rf && lin_bind.is_none() {
                lin_bind = Some(b.span);
                self.note_held(cx, b.span, &ty);
            }
            self.exit_scope(cx, saved, rig.times(factor), "E0303", "an ω let binding");
            if rig != Mult::Zero {
                self.check_ubind(cx, m, &ty, b.span, "let binding");
            }
            let val = self.eval(cx, &vt);
            let at = self.quote(cx, &ty);
            let l = self.push(cx, &b.name, m, &ty, Some(val), b.span, Some(&b.name));
            if let (Some(h), None, Mult::One, false) = (held_at, b.mult, m, uses_linear)
                && self.univ_of(cx, &ty) == Univ::Type
            {
                cx.holds.insert(l, h);
            }
            binds.push((Name::from(b.name.as_str()), m, at, vt));
        }
        let (bt, bty) = match expected {
            Some(ty) => {
                // `ty` lives in the outer context; values are closed, so it
                // can be used directly.
                (self.check(cx, body, ty, rig), ty.clone())
            }
            None => self.infer(cx, body, rig),
        };
        // The result may hold a linear binder (in a closure or an action).
        if let Some(at) = lin_bind
            && rig != Mult::Zero
            && self.may_hide_linear(cx, &bty)
        {
            cx.holders.push(at);
        }
        for _ in 0..binds.len() {
            self.pop(cx, rig != Mult::Zero);
        }
        self.exit_scope(
            cx,
            outer_saved,
            outer_rig,
            "E0303",
            "a `let` used many times",
        );
        let mut t = bt;
        for (x, m, a, v) in binds.into_iter().rev() {
            t = Term::Let(x, m, Rc::new(a), Rc::new(v), Rc::new(t));
        }
        (t, bty)
    }

    // ---- sugar ----

    fn gexpr(&self, g: Option<GName>, what: &str, span: Span) -> Option<rast::Expr> {
        self.builtin(g, what, span)
            .map(|g| mk(ExprKind::Global(g), span))
    }

    fn desugar_if(
        &self,
        c: &rast::Expr,
        t: &rast::Expr,
        f: &rast::Expr,
        span: Span,
    ) -> Option<rast::Expr> {
        let tr = self.builtin(self.prog.builtins.true_, "True", span)?;
        let fa = self.builtin(self.prog.builtins.false_, "False", span)?;
        let pat = |g| rast::Pat {
            kind: PatKind::Con(g, Vec::new()),
            span,
        };
        Some(mk(
            ExprKind::Case(
                Box::new(c.clone()),
                vec![
                    rast::Alt {
                        pat: pat(tr),
                        rhs: t.clone(),
                    },
                    rast::Alt {
                        pat: pat(fa),
                        rhs: f.clone(),
                    },
                ],
            ),
            span,
        ))
    }

    /// In `IO` statements are sequenced by `primBind`, whose continuation
    /// is linear (so `do` blocks may hold linear resources); other monads
    /// use the method `>>=`.
    fn desugar_do(&self, ss: &[rast::Stmt], span: Span, io: bool) -> Option<rast::Expr> {
        let bind = if io {
            (self.prog.builtins.prim_bind, "primBind")
        } else {
            (self.prog.builtins.bind, ">>=")
        };
        let Some((s, rest)) = ss.split_first() else {
            self.report(self.error("E1002", span, "empty `do` block"));
            return None;
        };
        if rest.is_empty() {
            return match s {
                rast::Stmt::Expr(e) => Some(e.clone()),
                _ => {
                    self.report(self.error(
                        "E1002",
                        span,
                        "the last statement of a `do` block must be an expression",
                    ));
                    None
                }
            };
        }
        let rest_span = span;
        let rest_e = mk(ExprKind::Do(rest.to_vec()), rest_span);
        if let rast::Stmt::Try(_, _, q) = s {
            self.report(
                self.error(
                    "E1260",
                    *q,
                    "`?` can only be used in a `do` block of `IO` or `IO1`",
                )
                .with_note("in other monads, `<-` already returns the error early"),
            );
            return None;
        }
        Some(match s {
            // `e; rest` is `e >>= \_ => rest`, not `e >> rest`: evaluation is
            // strict, so `rest` must stay under a lambda until `e` has run.
            rast::Stmt::Expr(e) => {
                let lam = mk(
                    ExprKind::Lam(
                        vec![rast::LamBinder {
                            name: None,
                            span: e.span,
                            mult: None,
                            ty: None,
                        }],
                        Box::new(rest_e),
                    ),
                    span,
                );
                app(app(self.gexpr(bind.0, bind.1, e.span)?, e.clone()), lam)
            }
            rast::Stmt::Bind(x, xs, e) => {
                let lam = mk(
                    ExprKind::Lam(
                        vec![rast::LamBinder {
                            name: Some(x.clone()),
                            span: *xs,
                            mult: None,
                            ty: None,
                        }],
                        Box::new(rest_e),
                    ),
                    span,
                );
                app(app(self.gexpr(bind.0, bind.1, e.span)?, e.clone()), lam)
            }
            rast::Stmt::Let(bs) => mk(ExprKind::Let(bs.clone(), Box::new(rest_e)), span),
            rast::Stmt::Try(..) => return None,
        })
    }

    /// `Some(false)` for `IO b`, `Some(true)` for `IO1 b`.
    fn io_kind(&self, ty: &Val) -> Option<bool> {
        let b = &self.prog.builtins;
        match self.whnf(ty) {
            Val::Neutral(Head::Global(g), sp) if sp.len() == 1 && Some(g) == b.io => Some(false),
            Val::Neutral(Head::Global(g), sp) if sp.len() == 1 && Some(g) == b.io1 => Some(true),
            _ => None,
        }
    }

    /// The result type of `IO b` / `IO1 b`.
    fn io_result(&self, ty: &Val) -> Val {
        match self.whnf(ty) {
            Val::Neutral(_, sp) => match sp.first() {
                Some(Elim::App(a, _)) => a.clone(),
                _ => Val::Error,
            },
            _ => Val::Error,
        }
    }

    /// A `do` block of `IO b` / `IO1 b` (spec-v0.0.2.md §5.2). Each action
    /// is elaborated first: the result of an `IO` action is bound with
    /// multiplicity ω and that of an `IO1` action with 1, and the bind is
    /// chosen by the kinds of the action and of the block.
    fn elab_io_do(&self, cx: &mut Cxt, ss: &[rast::Stmt], ty: &Val, rig: Mult, span: Span) -> Term {
        let Some((s, rest)) = ss.split_first() else {
            self.report(self.error("E1002", span, "empty `do` block"));
            return Term::Error;
        };
        if rest.is_empty() {
            return match s {
                rast::Stmt::Expr(e) => self.check(cx, e, ty, rig),
                _ => {
                    self.report(self.error(
                        "E1002",
                        span,
                        "the last statement of a `do` block must be an expression",
                    ));
                    Term::Error
                }
            };
        }
        let rest_e = mk(ExprKind::Do(rest.to_vec()), span);
        match s {
            rast::Stmt::Let(bs) => self.elab_let(cx, bs, &rest_e, Some(ty), rig).0,
            rast::Stmt::Expr(e) => self.io_bind(cx, None, e, None, &rest_e, ty, rig),
            rast::Stmt::Bind(x, xs, e) => {
                self.io_bind(cx, Some((x.as_str(), *xs)), e, None, &rest_e, ty, rig)
            }
            rast::Stmt::Try(x, e, q) => {
                let x = x.as_ref().map(|(n, sp)| (n.as_str(), *sp));
                self.io_bind(cx, x, e, Some(*q), &rest_e, ty, rig)
            }
        }
    }

    /// `x <- e; rest` (or `x <- e?; rest` when `try_at` is set) in a block
    /// of type `ty`.
    #[allow(clippy::too_many_arguments)]
    fn io_bind(
        &self,
        cx: &mut Cxt,
        x: Option<(&str, Span)>,
        e: &rast::Expr,
        try_at: Option<Span>,
        rest: &rast::Expr,
        ty: &Val,
        rig: Mult,
    ) -> Term {
        let b = &self.prog.builtins;
        let block1 = self.io_kind(ty) == Some(true);
        let bty = self.io_result(ty);
        let (et, ety) = self.infer(cx, e, rig);
        let (et, ety) = self.insert_implicits(cx, et, ety, e.span);
        if matches!(self.whnf(&ety), Val::Error) {
            return Term::Error;
        }
        // `e?` on a `Result` value (not an action): `case e of ...`.
        if let Some(q) = try_at
            && self.io_kind(&ety).is_none()
            && self.is_result(&ety)
        {
            let m = if self.univ_of(cx, &ety) == Univ::LType {
                Mult::One
            } else {
                Mult::Many
            };
            let at = self.quote(cx, &ety);
            let ev = self.eval(cx, &et);
            self.push(cx, "r$try", m, &ety, Some(ev), e.span, Some("r$try"));
            let body = self.try_case(cx, x, q, rest, ty, block1, rig);
            // The action may hold the fields of a linear result.
            if m == Mult::One && rig != Mult::Zero && !self.resource_free(cx, &ety) {
                cx.holders.push(e.span);
                self.note_held(cx, e.span, &ety);
            }
            self.pop(cx, rig != Mult::Zero);
            return Term::Let("r$try".into(), m, Rc::new(at), Rc::new(et), Rc::new(body));
        }
        let stmt1 = match self.io_kind(&ety) {
            Some(k) => k,
            None => {
                // An action whose type is not known yet is an `IO` action.
                let Some(io) = b.io else {
                    return Term::Error;
                };
                let a = self.fresh_meta(cx, e.span);
                let av = self.eval(cx, &a);
                let want = Val::Neutral(Head::Global(io), vec![Elim::App(av, Icit::Expl)]);
                if !self.unify_or_err(cx, e.span, &want, &ety) {
                    return Term::Error;
                }
                false
            }
        };
        let a = self.io_result(&ety);
        let bind = match (stmt1, block1) {
            (false, false) => b.prim_bind,
            (true, false) => b.prim_bind1,
            (false, true) => b.prim_bind_to1,
            (true, true) => b.prim_bind11,
        };
        let Some(bind) = self.builtin(bind, "primBind", e.span) else {
            return Term::Error;
        };
        let xm = if stmt1 { Mult::One } else { Mult::Many };
        let (name, xspan, src): (Name, Span, Option<&str>) = match (try_at, x) {
            (Some(_), _) => ("r$try".into(), e.span, Some("r$try")),
            (None, Some((n, sp))) => (n.into(), sp, Some(n)),
            (None, None) => ("_".into(), e.span, None),
        };
        if rig != Mult::Zero {
            self.check_ubind(cx, xm, &a, xspan, "result");
        }
        self.push(cx, &name, xm, &a, None, xspan, src);
        // The continuation runs at each run of the action: the linear values
        // it creates are not shared.
        let held = std::mem::take(&mut cx.holders);
        let body = match try_at {
            Some(q) => {
                if self.is_result(&a) {
                    self.try_case(cx, x, q, rest, ty, block1, rig)
                } else {
                    self.report(
                        self.error("E1260", q, "`?` needs a value or an action of `Result`")
                            .with_note(format!("the action returns `{}`", self.show(cx, &a))),
                    );
                    Term::Error
                }
            }
            None => self.check(cx, rest, ty, rig),
        };
        cx.holders = held;
        self.pop(cx, rig != Mult::Zero);
        let aq = self.quote(cx, &a);
        let bq = self.quote(cx, &bty);
        let f = Term::App(
            Rc::new(Term::App(
                Rc::new(Term::Global(bind)),
                Rc::new(aq.clone()),
                Icit::Impl,
            )),
            Rc::new(bq),
            Icit::Impl,
        );
        Term::App(
            Rc::new(Term::App(Rc::new(f), Rc::new(et), Icit::Expl)),
            Rc::new(Term::Lam(name, xm, Icit::Expl, Rc::new(aq), Rc::new(body))),
            Icit::Expl,
        )
    }

    fn is_result(&self, ty: &Val) -> bool {
        matches!(self.whnf(ty), Val::Data(d, _) if Some(d) == self.prog.builtins.result)
    }

    /// `case r$try of { Err e => pure (Err e); Ok x => rest }` in the
    /// context where `r$try` is bound (spec-v0.0.2.md §6.2).
    #[allow(clippy::too_many_arguments)]
    fn try_case(
        &self,
        cx: &mut Cxt,
        x: Option<(&str, Span)>,
        q: Span,
        rest: &rast::Expr,
        ty: &Val,
        block1: bool,
        rig: Mult,
    ) -> Term {
        let b = &self.prog.builtins;
        if !self.is_result(&self.io_result(ty))
            && !matches!(
                self.whnf(&self.io_result(ty)),
                Val::Neutral(Head::Meta(_), _)
            )
        {
            self.report(
                self.error(
                    "E1260",
                    q,
                    "`?` returns an error early, but this block does not return `Result`",
                )
                .with_note(format!("the block has type `{}`", self.show(cx, ty))),
            );
            return Term::Error;
        }
        let (Some(err), Some(ok)) = (b.err, b.ok) else {
            self.builtin(None, "Result", q);
            return Term::Error;
        };
        let pure = if block1 { b.prim_pure1 } else { b.prim_pure };
        let Some(pure) = self.gexpr(pure, "primPure", q) else {
            return Term::Error;
        };
        let var = |n: &str| mk(ExprKind::Local(n.into()), q);
        let pat = |kind| rast::Pat { kind, span: q };
        let ok_pat = match x {
            Some((n, _)) => pat(PatKind::Var(n.into())),
            None => pat(PatKind::Wild),
        };
        let alts = vec![
            rast::Alt {
                pat: pat(PatKind::Con(err, vec![pat(PatKind::Var("e$try".into()))])),
                rhs: app(pure, app(mk(ExprKind::Global(err), q), var("e$try"))),
            },
            rast::Alt {
                pat: pat(PatKind::Con(ok, vec![ok_pat])),
                rhs: rest.clone(),
            },
        ];
        let c = mk(ExprKind::Case(Box::new(var("r$try")), alts), q);
        self.check(cx, &c, ty, rig)
    }

    /// Tuple and list values.
    fn desugar_value(&self, e: &rast::Expr) -> Option<rast::Expr> {
        let span = e.span;
        match &e.kind {
            ExprKind::Tuple(es) => {
                let mk_pair = self.gexpr(self.prog.builtins.mk_pair, "MkPair", span)?;
                let mut it = es.iter().rev();
                let mut acc = it.next()?.clone();
                for x in it {
                    acc = app(app(mk_pair.clone(), x.clone()), acc);
                }
                Some(acc)
            }
            ExprKind::List(es) => {
                let nil = self.gexpr(self.prog.builtins.nil, "Nil", span)?;
                let cons = self.gexpr(self.prog.builtins.cons, "::", span)?;
                let mut acc = nil;
                for x in es.iter().rev() {
                    acc = app(app(cons.clone(), x.clone()), acc);
                }
                Some(acc)
            }
            _ => None,
        }
    }

    fn desugar_pat(&self, p: &rast::Pat) -> rast::Pat {
        let span = p.span;
        let con = |g: Option<GName>, what: &str, args: Vec<rast::Pat>| match self
            .builtin(g, what, span)
        {
            Some(g) => rast::Pat {
                kind: PatKind::Con(g, args),
                span,
            },
            None => rast::Pat {
                kind: PatKind::Wild,
                span,
            },
        };
        match &p.kind {
            PatKind::Unit => con(self.prog.builtins.mk_unit, "MkUnit", Vec::new()),
            PatKind::Tuple(ps) => {
                let ps: Vec<rast::Pat> = ps.iter().map(|x| self.desugar_pat(x)).collect();
                let mut it = ps.into_iter().rev();
                let mut acc = it.next().unwrap_or(rast::Pat {
                    kind: PatKind::Wild,
                    span,
                });
                for x in it {
                    acc = con(self.prog.builtins.mk_pair, "MkPair", vec![x, acc]);
                }
                acc
            }
            PatKind::List(ps) => {
                let mut acc = con(self.prog.builtins.nil, "Nil", Vec::new());
                for x in ps.iter().rev() {
                    acc = con(
                        self.prog.builtins.cons,
                        "::",
                        vec![self.desugar_pat(x), acc],
                    );
                }
                acc
            }
            PatKind::Con(g, ps) => rast::Pat {
                kind: PatKind::Con(*g, ps.iter().map(|x| self.desugar_pat(x)).collect()),
                span,
            },
            _ => p.clone(),
        }
    }

    // ---- case expressions and clauses ----

    fn elab_case(
        &self,
        cx: &mut Cxt,
        s: &rast::Expr,
        alts: &[rast::Alt],
        ty: &Val,
        rig: Mult,
        span: Span,
    ) -> Term {
        let mut ci = ClauseInfo {
            what: "case".into(),
            used: vec![false; alts.len()],
            spans: alts.iter().map(|a| a.pat.span).collect(),
            top: Vec::new(),
            span,
        };
        // Matching on a variable refines it in the branches.
        if let ExprKind::Local(x) = &s.kind
            && let Some(l) = cx.lookup(x)
            && cx.bound[l as usize]
        {
            ci.top = vec![l];
            let rows = alts
                .iter()
                .enumerate()
                .map(|(i, a)| Row {
                    pats: vec![(l, self.desugar_pat(&a.pat))],
                    binds: Vec::new(),
                    rhs: Rc::new(a.rhs.clone()),
                    clause: i,
                })
                .collect();
            let goal = self.quote(cx, ty);
            let t = self.compile(cx, rows, &goal, rig, &mut ci);
            self.warn_unused(&ci);
            return t;
        }
        let saved = self.enter_scope(cx);
        let inner = if rig == Mult::Zero {
            Mult::Zero
        } else {
            Mult::One
        };
        let (st, sty) = self.infer(cx, s, inner);
        let (st, sty) = self.insert_implicits(cx, st, sty, s.span);
        let rf = self.resource_free(cx, &sty);
        // A scrutinee that holds a linear value is linear (its fields too).
        let m = if self.univ_of(cx, &sty) == Univ::LType || (!cx.holders.is_empty() && !rf) {
            Mult::One
        } else {
            Mult::Many
        };
        let factor = if m == Mult::Many && rf { Mult::One } else { m };
        cx.holders.clear();
        self.exit_scope(cx, saved, rig.times(factor), "E0303", "a shared scrutinee");
        let val = self.eval(cx, &st);
        let at = self.quote(cx, &sty);
        let l = self.push(cx, "scrut$", m, &sty, Some(val), s.span, None);
        ci.top = vec![l];
        let rows = alts
            .iter()
            .enumerate()
            .map(|(i, a)| Row {
                pats: vec![(l, self.desugar_pat(&a.pat))],
                binds: Vec::new(),
                rhs: Rc::new(a.rhs.clone()),
                clause: i,
            })
            .collect();
        let goal = self.quote(cx, ty);
        let body = self.compile(cx, rows, &goal, rig, &mut ci);
        // The result may hold the linear scrutinee's fields.
        if m == Mult::One && !rf && rig != Mult::Zero && self.may_hide_linear(cx, ty) {
            cx.holders.push(s.span);
            self.note_held(cx, s.span, &sty);
        }
        self.pop(cx, rig != Mult::Zero);
        self.warn_unused(&ci);
        Term::Let("scrut$".into(), m, Rc::new(at), Rc::new(st), Rc::new(body))
    }

    fn warn_unused(&self, ci: &ClauseInfo) {
        for (i, u) in ci.used.iter().enumerate() {
            if !u {
                self.report(
                    Diagnostic::warning(if ci.what == "case" {
                        "unreachable alternative".to_string()
                    } else {
                        format!("unreachable clause of `{}`", ci.what)
                    })
                    .with_code("W1231")
                    .with_label(ci.spans[i], "never matched"),
                );
            }
        }
    }

    /// Elaborates a definition given by clauses against `ty`.
    fn elab_clauses(
        &self,
        cx: &mut Cxt,
        name: &str,
        clauses: &[rast::Clause],
        ty: &Val,
        span: Span,
    ) -> Term {
        let n = clauses.first().map(|c| c.pats.len()).unwrap_or(0);
        for c in clauses {
            if c.pats.len() != n {
                self.report(self.error(
                    "E1200",
                    c.span,
                    format!("the clauses of `{name}` have different numbers of arguments"),
                ));
                return Term::Error;
            }
        }
        if n == 0 {
            let t = self.check(cx, &clauses[0].rhs, ty, Mult::One);
            for c in &clauses[1..] {
                self.report(
                    Diagnostic::warning(format!("unreachable clause of `{name}`"))
                        .with_code("W1231")
                        .with_label(c.span, "never matched"),
                );
            }
            return t;
        }
        let mut lams = Vec::new();
        let mut cols = Vec::new();
        let mut t = ty.clone();
        while cols.len() < n {
            match self.whnf(&t) {
                Val::Pi(x, m, i, a, c) => {
                    let mut bname: Name = x.clone();
                    let mut bspan = span;
                    if i == Icit::Expl {
                        let p = &clauses[0].pats[cols.len()];
                        bspan = p.span;
                        bname = match &p.kind {
                            PatKind::Var(v) => v.as_str().into(),
                            _ if x.as_ref() == "_" => format!("x{}", cols.len()).into(),
                            _ => x.clone(),
                        };
                        self.check_ubind(cx, m, &a, bspan, "parameter");
                    }
                    let l = self.push(cx, &bname, m, &a, None, bspan, None);
                    if i == Icit::Expl {
                        cols.push(l);
                    }
                    lams.push((bname, m, i, self.ev().quote(l, &a)));
                    t = self.ev().inst(&c, Val::var(l));
                }
                Val::Error => return Term::Error,
                other => {
                    self.report(
                        self.error(
                            "E1204",
                            clauses[0].span,
                            format!(
                                "`{name}` is defined with {n} arguments, but its type has only {}",
                                cols.len()
                            ),
                        )
                        .with_note(format!("the remaining type is `{}`", self.show(cx, &other))),
                    );
                    for _ in 0..lams.len() {
                        self.pop(cx, false);
                    }
                    return Term::Error;
                }
            }
        }
        let goal = self.quote(cx, &t);
        let rows = clauses
            .iter()
            .enumerate()
            .map(|(i, c)| Row {
                pats: cols
                    .iter()
                    .copied()
                    .zip(c.pats.iter().map(|p| self.desugar_pat(p)))
                    .collect(),
                binds: Vec::new(),
                rhs: Rc::new(c.rhs.clone()),
                clause: i,
            })
            .collect();
        let mut ci = ClauseInfo {
            what: name.to_string(),
            used: vec![false; clauses.len()],
            spans: clauses.iter().map(|c| c.span).collect(),
            top: cols,
            span,
        };
        let mut body = self.compile(cx, rows, &goal, Mult::One, &mut ci);
        for _ in 0..lams.len() {
            self.pop(cx, true);
        }
        self.warn_unused(&ci);
        for (x, m, i, a) in lams.into_iter().rev() {
            body = Term::Lam(x, m, i, Rc::new(a), Rc::new(body));
        }
        body
    }

    fn example(&self, cx: &Cxt, ci: &ClauseInfo) -> String {
        fn show(cx: &Cxt, l: u32) -> String {
            match cx.shapes.get(&l) {
                Some(Shape::Con(c, fs)) if fs.is_empty() => c.clone(),
                Some(Shape::Con(c, fs)) => {
                    let a: Vec<String> = fs.iter().map(|f| show(cx, *f)).collect();
                    format!("({c} {})", a.join(" "))
                }
                Some(Shape::Lit(s)) => s.clone(),
                None => "_".into(),
            }
        }
        let a: Vec<String> = ci.top.iter().map(|l| show(cx, *l)).collect();
        if ci.what == "case" {
            a.join(" ")
        } else {
            format!(
                "{} {}",
                ci.what.rsplit('.').next().unwrap_or(&ci.what),
                a.join(" ")
            )
        }
    }

    fn compile(
        &self,
        cx: &mut Cxt,
        mut rows: Vec<Row>,
        goal: &Term,
        rig: Mult,
        ci: &mut ClauseInfo,
    ) -> Term {
        if rows.is_empty() {
            // An empty match on a value whose type has no possible
            // constructor (e.g. `case h of {}` on an empty data type).
            for &x in &ci.top.clone() {
                if cx.bound[x as usize] && !cx.shapes.contains_key(&x) && self.absurd(cx, x) {
                    self.use_var(cx, x, rig, ci.span);
                    return Term::Case(Rc::new(Case {
                        scrut: Term::Var(cx.ix(x)),
                        ret: goal.clone(),
                        alts: Vec::new(),
                        default: None,
                    }));
                }
            }
            let ex = self.example(cx, ci);
            self.report(
                self.error("E1230", ci.span, "patterns are not exhaustive")
                    .with_note(format!("missing case: `{ex}`"))
                    .with_help(
                        "add a clause for the missing case (use `panic` for impossible cases)",
                    ),
            );
            return Term::Error;
        }
        // Strip variables and wildcards of the first row.
        {
            let r0 = &mut rows[0];
            let mut i = 0;
            while i < r0.pats.len() {
                match &r0.pats[i].1.kind {
                    PatKind::Var(v) => {
                        r0.binds.push((v.clone(), r0.pats[i].0));
                        r0.pats.remove(i);
                    }
                    PatKind::Wild => {
                        r0.pats.remove(i);
                    }
                    _ => i += 1,
                }
            }
        }
        let Some((x, p)) = rows[0].pats.first().cloned() else {
            let r0 = rows.swap_remove(0);
            ci.used[r0.clause] = true;
            let n = cx.scope.len();
            for (v, l) in &r0.binds {
                cx.scope.push((v.clone(), *l));
            }
            let g = self.eval(cx, goal);
            let t = self.check(cx, &r0.rhs, &g, rig);
            cx.scope.truncate(n);
            return t;
        };
        let mut xty = self.whnf(&self.ty_val(cx, x));
        if matches!(xty, Val::Neutral(Head::Meta(_), _))
            && (self.solve_pending(false) || self.default_literals())
        {
            self.solve_pending(false);
            xty = self.whnf(&self.ty_val(cx, x));
        }
        // Nat literal patterns become constructor patterns.
        if let Val::Data(d, _) = &xty
            && Some(*d) == self.prog.builtins.nat
        {
            for r in &mut rows {
                for (_, q) in &mut r.pats {
                    nat_pat(&self.prog.builtins, q);
                }
            }
        }
        let p = rows[0]
            .pats
            .iter()
            .find(|(l, _)| *l == x)
            .map(|q| q.1.clone())
            .unwrap_or(p);
        match &p.kind {
            PatKind::Con(..) => self.split_con(cx, rows, x, &xty, goal, rig, ci, p.span),
            PatKind::Lit(..) => self.split_lit(cx, rows, x, &xty, goal, rig, ci),
            _ => unreachable!("stripped above"),
        }
    }

    /// Every constructor of the type of `x` is impossible.
    fn absurd(&self, cx: &Cxt, x: u32) -> bool {
        let Val::Data(d, args) = self.whnf(&self.ty_val(cx, x)) else {
            return false;
        };
        let Global::Data { cons, nparams, .. } = self.prog.get(d).clone() else {
            return false;
        };
        let params: Vec<Val> = args.iter().take(nparams).map(|a| a.0.clone()).collect();
        let indices: Vec<Val> = args.iter().skip(nparams).map(|a| a.0.clone()).collect();
        for c in cons {
            let mut bcx = cx.clone();
            let Some(cty) = self.prog.type_of(c) else {
                return false;
            };
            let mut ct = self.ev().eval(&Vec::new(), cty);
            for p in &params {
                if let Val::Pi(_, _, _, _, clo) = self.whnf(&ct) {
                    ct = self.ev().inst(&clo, p.clone());
                }
            }
            while let Val::Pi(fx, fm, _, fa, clo) = self.whnf(&ct) {
                let l = self.push(&mut bcx, &fx, fm, &fa, None, Span::DUMMY, None);
                ct = self.ev().inst(&clo, Val::var(l));
            }
            let Val::Data(_, rargs) = self.whnf(&ct) else {
                return false;
            };
            let pairs: Vec<(Term, Term)> = rargs
                .iter()
                .skip(nparams)
                .zip(&indices)
                .map(|(a, b)| (self.quote(&bcx, &a.0), self.quote(&bcx, b)))
                .collect();
            if !matches!(
                self.match_unify(&mut bcx, pairs, &mut Vec::new()),
                Unif::Absurd
            ) {
                return false;
            }
        }
        true
    }

    /// Weakens `goal` (in the context of `base` variables) to `cx`.
    fn weaken_goal(&self, cx: &Cxt, base: usize, goal: &Term) -> Term {
        let env: Env = cx.env[..base].to_vec();
        let v = self.ev().eval(&env, goal);
        self.quote(cx, &v)
    }

    #[allow(clippy::too_many_arguments)]
    fn split_con(
        &self,
        cx: &mut Cxt,
        rows: Vec<Row>,
        x: u32,
        xty: &Val,
        goal: &Term,
        rig: Mult,
        ci: &mut ClauseInfo,
        pspan: Span,
    ) -> Term {
        let Val::Data(d, args) = xty else {
            if !matches!(xty, Val::Error) {
                self.report(
                    self.error(
                        "E1200",
                        pspan,
                        "constructor pattern for a value that is not of a data type",
                    )
                    .with_note(format!("the value has type `{}`", self.show(cx, xty))),
                );
            }
            return Term::Error;
        };
        let Global::Data { cons, nparams, .. } = self.prog.get(*d).clone() else {
            return Term::Error;
        };
        let params: Vec<Val> = args.iter().take(nparams).map(|a| a.0.clone()).collect();
        let indices: Vec<Val> = args.iter().skip(nparams).map(|a| a.0.clone()).collect();
        // Constructor patterns of other types.
        for r in &rows {
            if let Some((_, q)) = r.pats.iter().find(|(l, _)| *l == x)
                && let PatKind::Con(c, _) = &q.kind
                && !cons.contains(c)
            {
                self.report(self.error(
                    "E1200",
                    q.span,
                    format!(
                        "`{}` is not a constructor of `{}`",
                        self.prog.short_name(*c),
                        self.prog.short_name(*d)
                    ),
                ));
                return Term::Error;
            }
        }
        let xm = cx.mults[x as usize];
        let base = cx.lvl() as usize;
        let before = cx.uses.clone();
        let mut alts = Vec::new();
        let mut results = Vec::new();
        for &c in &cons {
            let Global::Con {
                ty: cty,
                field_icits,
                ..
            } = self.prog.get(c).clone()
            else {
                continue;
            };
            let mut bcx = cx.clone();
            bcx.uses = before.clone();
            let mut ct = self.ev().eval(&Vec::new(), &cty);
            for p in &params {
                match self.whnf(&ct) {
                    Val::Pi(_, _, _, _, clo) => ct = self.ev().inst(&clo, p.clone()),
                    _ => break,
                }
            }
            // Field names from the first row that matches this constructor.
            let pat_names: Vec<Option<String>> = rows
                .iter()
                .find_map(
                    |r| match r.pats.iter().find(|(l, _)| *l == x).map(|q| &q.1.kind) {
                        Some(PatKind::Con(c2, ps)) if *c2 == c => Some(
                            ps.iter()
                                .map(|q| match &q.kind {
                                    PatKind::Var(v) => Some(v.clone()),
                                    _ => None,
                                })
                                .collect(),
                        ),
                        _ => None,
                    },
                )
                .unwrap_or_default();
            let mut fields = Vec::new();
            let mut field_lvls = Vec::new();
            let mut expl_lvls = Vec::new();
            for (k, fi) in field_icits.iter().enumerate() {
                let Val::Pi(fx, fm, _, fa, clo) = self.whnf(&ct) else {
                    break;
                };
                let m = match (xm, fm) {
                    (Mult::Zero, _) | (_, Mult::Zero) => Mult::Zero,
                    (Mult::One, _) => Mult::One,
                    (Mult::Many, _) => Mult::Many,
                };
                let name: Name = if *fi == Icit::Expl {
                    match pat_names.get(expl_lvls.len()).cloned().flatten() {
                        Some(v) => v.into(),
                        None if fx.as_ref() == "_" => format!(
                            "{}{k}",
                            self.prog
                                .short_name(c)
                                .to_lowercase()
                                .chars()
                                .next()
                                .unwrap_or('f')
                        )
                        .into(),
                        None => fx.clone(),
                    }
                } else {
                    fx.clone()
                };
                let l = self.push(&mut bcx, &name, m, &fa, None, pspan, None);
                fields.push((name, m, self.ev().quote(l, &fa)));
                field_lvls.push(l);
                if *fi == Icit::Expl {
                    expl_lvls.push(l);
                }
                ct = self.ev().inst(&clo, Val::var(l));
            }
            // Index unification.
            let mut refined = Vec::new();
            match self.whnf(&ct) {
                Val::Data(_, rargs) => {
                    let pairs: Vec<(Term, Term)> = rargs
                        .iter()
                        .skip(nparams)
                        .zip(&indices)
                        .map(|(a, b)| (self.quote(&bcx, &a.0), self.quote(&bcx, b)))
                        .collect();
                    match self.match_unify(&mut bcx, pairs, &mut refined) {
                        Unif::Ok => {}
                        Unif::Absurd => continue,
                        Unif::Stuck(msg) => {
                            self.report(
                                self.error(
                                    "E1232",
                                    pspan,
                                    format!(
                                        "cannot unify the indices of `{}` in this match",
                                        self.prog.short_name(c)
                                    ),
                                )
                                .with_note(msg)
                                .with_help("make the index a variable or a constructor form"),
                            );
                            continue;
                        }
                    }
                }
                _ => continue,
            }
            let blvl = bcx.lvl();
            let refine: Vec<(u32, Term)> = refined
                .iter()
                .map(|l| (blvl - 1 - l, self.ev().quote(blvl, &bcx.env[*l as usize])))
                .collect();
            // x := C params fields.
            let mut con_t = Term::Global(c);
            for p in &params {
                con_t = Term::App(Rc::new(con_t), Rc::new(self.quote(&bcx, p)), Icit::Impl);
            }
            for (l, fi) in field_lvls.iter().zip(&field_icits) {
                con_t = Term::App(Rc::new(con_t), Rc::new(Term::Var(bcx.ix(*l))), *fi);
            }
            let con_v = self.eval(&bcx, &con_t);
            bcx.env[x as usize] = con_v;
            bcx.bound[x as usize] = false;
            bcx.shapes.insert(
                x,
                Shape::Con(self.prog.short_name(c).to_string(), expl_lvls.clone()),
            );
            // Specialize the rows.
            let mut brows = Vec::new();
            let mut bad = false;
            for r in &rows {
                let mut r = r.clone();
                match r.pats.iter().position(|(l, _)| *l == x) {
                    None => brows.push(r),
                    Some(i) => {
                        let (_, q) = r.pats.remove(i);
                        match q.kind {
                            PatKind::Con(c2, ps) if c2 == c => {
                                if ps.len() != expl_lvls.len() {
                                    self.report(self.error(
                                        "E1200",
                                        q.span,
                                        format!(
                                            "constructor `{}` expects {} arguments, but the pattern has {}",
                                            self.prog.short_name(c),
                                            expl_lvls.len(),
                                            ps.len()
                                        ),
                                    ));
                                    bad = true;
                                    continue;
                                }
                                let mut new: Vec<(u32, rast::Pat)> =
                                    expl_lvls.iter().copied().zip(ps).collect();
                                new.extend(r.pats);
                                r.pats = new;
                                brows.push(r);
                            }
                            PatKind::Con(..) => {}
                            PatKind::Var(v) => {
                                r.binds.push((v, x));
                                brows.push(r);
                            }
                            PatKind::Wild => brows.push(r),
                            _ => {
                                self.report(self.error(
                                    "E1200",
                                    q.span,
                                    "this pattern does not match a value of a data type",
                                ));
                                bad = true;
                            }
                        }
                    }
                }
            }
            if bad {
                return Term::Error;
            }
            let bgoal = self.weaken_goal(&bcx, base, goal);
            let xb = bcx.uses[x as usize].mult();
            let body = self.compile(&mut bcx, brows, &bgoal, rig, ci);
            let x_used = bcx.uses[x as usize].mult() != xb;
            let lin = xm == Mult::One && !self.resource_free(cx, &self.ty_val(cx, x));
            for _ in 0..field_lvls.len() {
                let fl = bcx.lvl() - 1;
                let u = self.pop(&mut bcx, !(lin && x_used));
                if lin && x_used && u.mult() != Mult::Zero {
                    let at = u.spans.first().copied().unwrap_or(pspan);
                    self.report(
                        self.error(
                            "E0303",
                            at,
                            format!(
                                "field `{}` is used although the matched value `{}` is also used",
                                fields[(fl as usize) - base].0,
                                cx.names[x as usize]
                            ),
                        )
                        .with_note(
                            "a linear value is consumed either by the match or by its other uses",
                        ),
                    );
                }
            }
            if !x_used {
                self.use_var(&mut bcx, x, rig, pspan);
            }
            results.push(bcx.uses.clone());
            alts.push(Alt::Con {
                con: c,
                fields,
                refine,
                body,
            });
        }
        if xm == Mult::Zero && rig != Mult::Zero && alts.len() > 1 {
            self.report(
                self.error(
                    "E0304",
                    pspan,
                    format!(
                        "cannot match on the erased variable `{}`",
                        cx.names[x as usize]
                    ),
                )
                .with_note("variables of multiplicity 0 do not exist at runtime")
                .with_help("give the binder multiplicity ω (the default) or 1"),
            );
        }
        self.join(cx, results, pspan);
        Term::Case(Rc::new(Case {
            scrut: Term::Var(cx.ix(x)),
            ret: goal.clone(),
            alts,
            default: None,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn split_lit(
        &self,
        cx: &mut Cxt,
        rows: Vec<Row>,
        x: u32,
        xty: &Val,
        goal: &Term,
        rig: Mult,
        ci: &mut ClauseInfo,
    ) -> Term {
        let b = &self.prog.builtins;
        let mut lits: Vec<Lit> = Vec::new();
        let conv = |q: &rast::Pat| -> Option<Lit> {
            let PatKind::Lit(l, neg) = &q.kind else {
                return None;
            };
            let r = match l {
                SLit::Int(n) if self.is_prim_type(xty, b.int) => {
                    let v = if *neg { -(*n as i128) } else { *n as i128 };
                    (v >= i64::MIN as i128 && v <= i64::MAX as i128).then_some(Lit::Int(v as i64))
                }
                SLit::Int(n) if self.is_prim_type(xty, b.uint) && !neg => {
                    (*n <= u64::MAX as u128).then_some(Lit::UInt(*n as u64))
                }
                SLit::Int(n) if self.is_prim_type(xty, b.float) => {
                    Some(Lit::Float(if *neg { -(*n as f64) } else { *n as f64 }))
                }
                SLit::Float(f) if self.is_prim_type(xty, b.float) => {
                    Some(Lit::Float(if *neg { -f } else { *f }))
                }
                SLit::Char(c) if self.is_prim_type(xty, b.char_) => Some(Lit::Char(*c)),
                SLit::Str(s) if self.is_prim_type(xty, b.string) => {
                    Some(Lit::Str(s.as_str().into()))
                }
                _ => None,
            };
            if r.is_none() {
                self.report(
                    self.error(
                        "E1200",
                        q.span,
                        "literal pattern does not match the type of the value",
                    )
                    .with_note(format!("the value has type `{}`", self.show(cx, xty))),
                );
            }
            r
        };
        let mut rows2 = Vec::new();
        for r in &rows {
            let mut r = r.clone();
            let mut keep = true;
            for (l, q) in &mut r.pats {
                if *l == x && matches!(q.kind, PatKind::Lit(..)) {
                    match conv(q) {
                        Some(lit) => {
                            if !lits.contains(&lit) {
                                lits.push(lit.clone());
                            }
                            q.kind = PatKind::Lit(SLit::Str(format!("{lit}")), false);
                        }
                        None => keep = false,
                    }
                }
            }
            if !keep {
                return Term::Error;
            }
            rows2.push(r);
        }
        let key = |q: &rast::Pat| match &q.kind {
            PatKind::Lit(SLit::Str(s), _) => Some(s.clone()),
            _ => None,
        };
        let base = cx.lvl() as usize;
        let before = cx.uses.clone();
        let mut results = Vec::new();
        let mut alts = Vec::new();
        let branch = |bcx: &mut Cxt, want: Option<&Lit>, ci: &mut ClauseInfo| -> Term {
            let mut brows = Vec::new();
            for r in &rows2 {
                let mut r = r.clone();
                match r.pats.iter().position(|(l, _)| *l == x) {
                    None => brows.push(r),
                    Some(i) => {
                        let k = key(&r.pats[i].1);
                        match (k, want) {
                            (Some(k), Some(w)) if k == format!("{w}") => {
                                r.pats.remove(i);
                                brows.push(r);
                            }
                            (Some(_), _) => {}
                            (None, _) => {
                                let (_, q) = r.pats.remove(i);
                                if let PatKind::Var(v) = q.kind {
                                    r.binds.push((v, x));
                                }
                                brows.push(r);
                            }
                        }
                    }
                }
            }
            let bgoal = self.weaken_goal(bcx, base, goal);
            let xb = bcx.uses[x as usize].mult();
            let body = self.compile(bcx, brows, &bgoal, rig, ci);
            if bcx.uses[x as usize].mult() == xb {
                self.use_var(bcx, x, rig, ci.span);
            }
            body
        };
        for lit in &lits {
            let mut bcx = cx.clone();
            bcx.uses = before.clone();
            bcx.env[x as usize] = Val::Lit(lit.clone());
            bcx.bound[x as usize] = false;
            bcx.shapes.insert(x, Shape::Lit(format!("{lit}")));
            let body = branch(&mut bcx, Some(lit), ci);
            results.push(bcx.uses.clone());
            alts.push(Alt::Lit {
                lit: lit.clone(),
                body,
            });
        }
        let mut bcx = cx.clone();
        bcx.uses = before.clone();
        let default = branch(&mut bcx, None, ci);
        results.push(bcx.uses.clone());
        self.join(cx, results, ci.span);
        Term::Case(Rc::new(Case {
            scrut: Term::Var(cx.ix(x)),
            ret: goal.clone(),
            alts,
            default: Some(default),
        }))
    }

    /// Unifies constructor indices for dependent pattern matching,
    /// refining variables of the context.
    fn match_unify(&self, cx: &mut Cxt, pairs: Vec<(Term, Term)>, refined: &mut Vec<u32>) -> Unif {
        let mut todo: std::collections::VecDeque<(Term, Term)> = pairs.into();
        while let Some((a, b)) = todo.pop_front() {
            let va = self.whnf(&self.eval(cx, &a));
            let vb = self.whnf(&self.eval(cx, &b));
            let as_var = |v: &Val| match v {
                Val::Neutral(Head::Var(l), sp) if sp.is_empty() => Some(*l),
                _ => None,
            };
            let (la, lb) = (as_var(&va), as_var(&vb));
            let target = match (la, lb) {
                (Some(x), Some(y)) if x == y => continue,
                (Some(x), Some(y)) => Some((x.max(y), if x > y { vb.clone() } else { va.clone() })),
                (Some(x), None) => Some((x, vb.clone())),
                (None, Some(y)) => Some((y, va.clone())),
                (None, None) => None,
            };
            if let Some((l, v)) = target {
                let t = self.quote(cx, &v);
                if term_mentions(&t, cx.ix(l)) {
                    return Unif::Stuck(format!(
                        "`{}` occurs in `{}`",
                        cx.names[l as usize],
                        print_term(&self.prog, &mut cx.names.clone(), &t)
                    ));
                }
                cx.env[l as usize] = v;
                cx.bound[l as usize] = false;
                refined.push(l);
                continue;
            }
            match (&va, &vb) {
                (Val::Con(c1, a1), Val::Con(c2, a2)) => {
                    if c1 != c2 {
                        return Unif::Absurd;
                    }
                    for (x, y) in a1.iter().zip(a2) {
                        todo.push_back((self.quote(cx, x), self.quote(cx, y)));
                    }
                }
                (Val::Lit(x), Val::Lit(y)) => {
                    if x != y {
                        return Unif::Absurd;
                    }
                }
                (Val::Con(..), Val::Lit(_)) | (Val::Lit(_), Val::Con(..)) => return Unif::Absurd,
                _ => {
                    if self.ev().unify(cx.lvl(), &va, &vb).is_err() {
                        return Unif::Stuck(format!(
                            "`{}` and `{}` are not definitionally equal",
                            self.show(cx, &va),
                            self.show(cx, &vb)
                        ));
                    }
                }
            }
        }
        Unif::Ok
    }

    // ---- metas ----

    /// Replaces solved metas; reports unsolved ones (E1210).
    fn zonk(&self, lvl: u32, t: &Term) -> Term {
        let z = |l: u32, t: &Rc<Term>| Rc::new(self.zonk(l, t));
        match t {
            Term::Meta(m) | Term::InsertedMeta(m, _) => match self.metas.get(*m) {
                Some(mut v) => {
                    if let Term::InsertedMeta(_, mask) = t {
                        for (l, b) in mask.iter().enumerate() {
                            if *b {
                                v = self.ev().apply(v, Val::var(l as u32), Icit::Expl);
                            }
                        }
                    }
                    let q = self.ev().quote(lvl, &v);
                    self.zonk(lvl, &q)
                }
                None => {
                    let reported = &mut self.meta_reported.borrow_mut()[*m as usize];
                    if !*reported {
                        *reported = true;
                        let span = self.meta_spans.borrow()[*m as usize];
                        self.report(
                            self.error("E1210", span, "cannot infer this implicit argument or `_`")
                                .with_help("add a type annotation"),
                        );
                    }
                    Term::Error
                }
            },
            Term::Pi(x, m, i, a, b) => Term::Pi(x.clone(), *m, *i, z(lvl, a), z(lvl + 1, b)),
            Term::Lam(x, m, i, a, b) => Term::Lam(x.clone(), *m, *i, z(lvl, a), z(lvl + 1, b)),
            Term::App(f, a, i) => {
                // A solved meta applied to arguments: beta-reduce.
                let mut h: &Term = f;
                while let Term::App(g, _, _) = h {
                    h = g;
                }
                if let Term::Meta(m) | Term::InsertedMeta(m, _) = h
                    && self.metas.get(*m).is_some()
                {
                    let env: Env = (0..lvl).map(Val::var).collect();
                    let v = self.ev().eval(&env, t);
                    let q = self.ev().quote(lvl, &v);
                    return self.zonk(lvl, &q);
                }
                Term::App(z(lvl, f), z(lvl, a), *i)
            }
            Term::Let(x, m, a, v, b) => {
                Term::Let(x.clone(), *m, z(lvl, a), z(lvl, v), z(lvl + 1, b))
            }
            Term::Case(c) => Term::Case(Rc::new(Case {
                scrut: self.zonk(lvl, &c.scrut),
                ret: self.zonk(lvl, &c.ret),
                alts: c
                    .alts
                    .iter()
                    .map(|a| match a {
                        Alt::Con {
                            con,
                            fields,
                            refine,
                            body,
                        } => {
                            let n = fields.len() as u32;
                            Alt::Con {
                                con: *con,
                                fields: fields
                                    .iter()
                                    .enumerate()
                                    .map(|(k, (x, m, t))| {
                                        (x.clone(), *m, self.zonk(lvl + k as u32, t))
                                    })
                                    .collect(),
                                refine: refine
                                    .iter()
                                    .map(|(i, t)| (*i, self.zonk(lvl + n, t)))
                                    .collect(),
                                body: self.zonk(lvl + n, body),
                            }
                        }
                        Alt::Lit { lit, body } => Alt::Lit {
                            lit: lit.clone(),
                            body: self.zonk(lvl, body),
                        },
                    })
                    .collect(),
                default: c.default.as_ref().map(|d| self.zonk(lvl, d)),
            })),
            _ => t.clone(),
        }
    }

    // ---- top level ----

    pub fn run(&mut self) {
        let rp = self.rp;
        let items: Vec<&rast::Item> = rp.modules.iter().flat_map(|m| m.items.iter()).collect();
        let is_type_prim =
            |g: GName| Prim::from_name(&rp.info(g).name).is_some_and(|p| p.is_type());
        // 1. Primitive types and data type signatures.
        for it in &items {
            if let rast::Item::Prim(g, t) = it
                && is_type_prim(*g)
            {
                self.prim(*g, t);
            }
        }
        let traits: Vec<&rast::Trait> = items
            .iter()
            .filter_map(|it| match it {
                rast::Item::Trait(t) => Some(t),
                _ => None,
            })
            .collect();
        let impls: Vec<&rast::Impl> = items
            .iter()
            .filter_map(|it| match it {
                rast::Item::Impl(i) => Some(i),
                _ => None,
            })
            .collect();
        // (Constraints in signatures must be known to be traits.)
        for t in &traits {
            self.prog.traits.insert(
                t.name,
                TraitInfo {
                    con: t.dict_con,
                    supers: Vec::new(),
                    super_traits: Vec::new(),
                    defaults: Vec::new(),
                },
            );
        }
        for it in &items {
            if let rast::Item::Data(d) = it {
                self.data_sig(d);
            }
        }
        // 2. Other primitives; constructors, universes and Drop.
        for it in &items {
            if let rast::Item::Prim(g, t) = it
                && !is_type_prim(*g)
            {
                self.prim(*g, t);
            }
        }
        for it in &items {
            if let rast::Item::Data(d) = it {
                self.data_cons(d);
            }
        }
        // 3. Traits, the heads of implementations, signatures.
        for t in &traits {
            self.trait_item(t);
        }
        self.check_supertrait_cycles(&traits);
        for i in &impls {
            self.impl_sig(i);
        }
        self.check_coherence(&impls);
        for it in &items {
            if let rast::Item::Def(d) = it {
                self.def_sig(d);
            }
        }
        // 4. Bodies.
        for it in &items {
            match it {
                rast::Item::Def(d) => self.def_body(d),
                rast::Item::Trait(t) => self.default_bodies(t),
                rast::Item::Impl(i) => self.impl_body(i),
                _ => {}
            }
        }
        // `main : IO ()`.
        if let Some(m) = rp.main
            && let (Some(io), Some(unit)) = (self.prog.builtins.io, self.prog.builtins.unit)
            && let Some(ty) = self.global_type(m)
        {
            let want = Val::Neutral(
                Head::Global(io),
                vec![Elim::App(Val::Data(unit, Vec::new()), Icit::Expl)],
            );
            let cx = Cxt::default();
            if self.ev().unify(0, &want, &ty).is_err() {
                self.report(
                    self.error("E1200", rp.info(m).span, "`main` must have type `IO ()`")
                        .with_note(format!("its type is `{}`", self.show(&cx, &ty))),
                );
            }
        }
    }

    // ---- top-level values (spec-v0.0.2.md §5.1) ----

    /// A top-level value (a definition or a method that is not a function)
    /// is evaluated once and shared by all its uses: its type must not be
    /// linear.
    fn check_shared_type(&self, ty: &Term, span: Span, what: &str, name: &str, by: &str) {
        let mut cx = Cxt::default();
        let mut v = self.eval(&cx, ty);
        while let Val::Pi(x, Mult::Zero, _, a, c) = self.whnf(&v) {
            let l = self.push(&mut cx, &x, Mult::Zero, &a, None, span, None);
            v = self.ev().inst(&c, Val::var(l));
        }
        if matches!(self.whnf(&v), Val::Pi(..)) || self.univ_of(&cx, &v) != Univ::LType {
            return;
        }
        self.report(
            self.error(
                "E0301",
                span,
                format!(
                    "{what} has the linear type `{}`, but {by}",
                    self.show(&cx, &v)
                ),
            )
            .with_help(format!(
                "make it a function, e.g. `{name} : () -> {}`",
                self.show(&cx, &v)
            )),
        );
    }

    /// The body of a top-level value whose evaluation (outside runtime
    /// lambdas) holds linear values: an action is delayed, so that each run
    /// creates its own; anything else would share them (E0305).
    fn share_top(&self, cx: &mut Cxt, ty: &Term, body: Term) -> Term {
        let held = std::mem::take(&mut cx.holders);
        let Some(&at) = held.first() else {
            return body;
        };
        if matches!(body, Term::Error) {
            return body;
        }
        // Erased lambdas do not delay anything.
        let mut tcx = Cxt::default();
        let mut v = self.eval(&tcx, ty);
        let mut t = body;
        let mut lams = Vec::new();
        while let (Val::Pi(_, Mult::Zero, _, a, c), Term::Lam(x, Mult::Zero, i, at, b)) =
            (self.whnf(&v), &t)
        {
            let l = self.push(&mut tcx, x, Mult::Zero, &a, None, Span::DUMMY, None);
            v = self.ev().inst(&c, Val::var(l));
            lams.push((x.clone(), *i, at.clone()));
            let b = (**b).clone();
            t = b;
        }
        let b = &self.prog.builtins;
        let delayed = match self.io_kind(&v) {
            Some(lin) => {
                let r = self.quote(&tcx, &self.io_result(&v));
                let bind = if lin { b.prim_bind_to1 } else { b.prim_bind };
                match (bind, b.prim_pure, b.unit, b.mk_unit) {
                    (Some(bind), Some(pure), Some(unit), Some(mk_unit)) => {
                        let unit = Term::Global(unit);
                        let start = self.apply_global(
                            pure,
                            std::slice::from_ref(&unit),
                            &[Term::Global(mk_unit)],
                        );
                        let k = Term::Lam(
                            "_".into(),
                            Mult::Many,
                            Icit::Expl,
                            Rc::new(unit.clone()),
                            Rc::new(shift(&t, 0, 1)),
                        );
                        Some(self.apply_global(bind, &[unit, r], &[start, k]))
                    }
                    _ => None,
                }
            }
            None => None,
        };
        let Some(mut t) = delayed else {
            self.report(
                self.shared_holder(at, "the value of a top-level definition, shared by all its uses")
                    .with_help("write the parameters of the definition (e.g. `f x = ...`), so that the linear value is created at each call"),
            );
            return Term::Error;
        };
        for (x, i, a) in lams.into_iter().rev() {
            t = Term::Lam(x, Mult::Zero, i, a, Rc::new(t));
        }
        t
    }

    /// `g {imps} exps`, following the binders of the type of `g`.
    fn apply_global(&self, g: GName, imps: &[Term], exps: &[Term]) -> Term {
        let mut t = Term::Global(g);
        let mut ty = self.prog.type_of(g).cloned().unwrap_or(Term::Error);
        let (mut i, mut e) = (imps.iter(), exps.iter());
        while let Term::Pi(_, _, icit, _, b) = ty {
            let next = match icit {
                Icit::Impl => i.next(),
                Icit::Expl => e.next(),
            };
            let Some(a) = next else { break };
            t = Term::App(Rc::new(t), Rc::new(a.clone()), icit);
            ty = (*b).clone();
        }
        t
    }

    fn error_count(&self) -> usize {
        self.diags.borrow().iter().filter(|x| x.is_error()).count()
    }

    /// Solves what is left of the constraints of a definition and zonks
    /// its body (in context `cx`).
    fn finish(&self, cx: &Cxt, body: &Term) -> Term {
        self.solve_pending(true);
        self.zonk(cx.lvl(), body)
    }

    fn begin(&self, name: String) {
        self.shareable.borrow_mut().clear();
        self.fin_spans.borrow_mut().clear();
        self.fin_held.borrow_mut().clear();
        *self.current.borrow_mut() = name;
        self.fuel.set(FUEL);
        self.out_of_fuel.set(false);
        self.fuel_span.set(None);
    }

    fn data_sig(&mut self, d: &rast::Data) {
        self.begin(self.prog.name(d.name).to_string());
        let mut cx = Cxt::default();
        let mut ps = Vec::new();
        let mut lin_params = Vec::new();
        // The dictionary type of a trait keeps shareable parameters.
        let is_trait = self.prog.traits.contains_key(&d.name);
        for p in &d.params {
            // A parameter without a type ranges over `LType` and the data
            // type propagates its linearity (`Pair Int Int : Type`, `Pair
            // File Int` is linear).
            let a = match &p.ty {
                Some(t) => self.check_type(&mut cx, t),
                None if is_trait => Term::Type,
                None => Term::LType,
            };
            let av = self.eval(&cx, &a);
            lin_params.push(matches!(self.whnf(&av), Val::LType));
            let m = match p.mult {
                Some(m) => conv_mult(m),
                None if self.is_universe_valued(cx.lvl(), &av) => Mult::Zero,
                None => Mult::Many,
            };
            self.push(&mut cx, &p.name, m, &av, None, p.span, Some(&p.name));
            ps.push((Name::from(p.name.as_str()), m, a));
        }
        let sig = match &d.sig {
            Some(s) => self.check_type(&mut cx, s),
            None => Term::Type,
        };
        // The signature must end in a universe.
        let sv = self.eval(&cx, &sig);
        let mut univ = Univ::Type;
        {
            let mut v = self.whnf(&sv);
            let mut lvl = cx.lvl();
            loop {
                match v {
                    Val::Pi(_, _, _, _, c) => {
                        v = self.whnf(&self.ev().inst(&c, Val::var(lvl)));
                        lvl += 1;
                    }
                    Val::Type => break,
                    Val::LType => {
                        univ = Univ::LType;
                        break;
                    }
                    _ => {
                        self.report(self.error(
                            "E1201",
                            d.span,
                            "the type of a data type must end in `Type` or `LType`",
                        ));
                        break;
                    }
                }
            }
        }
        self.solve_pending(true);
        let mut ty = self.zonk(cx.lvl(), &sig);
        for (x, m, a) in ps.into_iter().rev() {
            ty = Term::Pi(x, m, Icit::Expl, Rc::new(a), Rc::new(ty));
        }
        let ty = self.zonk(0, &ty);
        self.prog.globals[d.name.0 as usize] = Global::Data {
            ty,
            nparams: d.params.len(),
            univ,
            cons: d.cons.iter().map(|c| c.name).collect(),
            lin_params,
        };
        self.prog.order.push(d.name);
    }

    fn data_cons(&mut self, d: &rast::Data) {
        self.begin(self.prog.name(d.name).to_string());
        let Global::Data {
            ty: dty,
            univ,
            nparams,
            lin_params,
            ..
        } = self.prog.get(d.name).clone()
        else {
            return;
        };
        let mut lin_field = false;
        for (tag, c) in d.cons.iter().enumerate() {
            let mut cx = Cxt::default();
            // Parameters, as implicit binders.
            let mut ps = Vec::new();
            let mut dt = self.ev().eval(&Vec::new(), &dty);
            for p in &d.params {
                let Val::Pi(x, _, _, a, clo) = self.whnf(&dt) else {
                    break;
                };
                let l = self.push(&mut cx, &x, Mult::Zero, &a, None, p.span, Some(&p.name));
                ps.push((x, self.ev().quote(l, &a)));
                dt = self.ev().inst(&clo, Val::var(l));
            }
            let sig = match &c.body {
                rast::ConBody::Sig(t) => self.check_type(&mut cx, t),
                rast::ConBody::Fields(fs) => {
                    let mut ts = Vec::new();
                    for (k, f) in fs.iter().enumerate() {
                        let t = self.check_type(&mut cx, f);
                        let tv = self.eval(&cx, &t);
                        self.push(&mut cx, "_", Mult::Many, &tv, None, f.span, None);
                        ts.push((k, t));
                    }
                    let mut res = Term::Global(d.name);
                    for k in 0..nparams {
                        res = Term::App(
                            Rc::new(res),
                            Rc::new(Term::Var(cx.ix(k as u32))),
                            Icit::Expl,
                        );
                    }
                    let mut t = res;
                    for (_, ft) in ts.into_iter().rev() {
                        self.pop(&mut cx, false);
                        t = Term::Pi("_".into(), Mult::Many, Icit::Expl, Rc::new(ft), Rc::new(t));
                    }
                    t
                }
            };
            self.solve_pending(true);
            let sig = self.zonk(cx.lvl(), &sig);
            // Walk the fields.
            let mut v = self.eval(&cx, &sig);
            let mut mults = Vec::new();
            let mut icits = Vec::new();
            let mut fcx = cx.clone();
            // The universe of the data type itself is computed with the
            // propagating parameters taken as shareable.
            for (k, lin) in lin_params.iter().enumerate() {
                if *lin && k < fcx.tys.len() {
                    fcx.tys[k] = Term::Type;
                }
            }
            loop {
                match self.whnf(&v) {
                    Val::Pi(x, m, i, a, clo) => {
                        if m != Mult::Zero && self.univ_of(&fcx, &a) == Univ::LType {
                            lin_field = true;
                            if univ == Univ::Type && !d.short {
                                self.report(
                                    self.error("E1203", c.span, format!("`{}` is declared in `Type` but has a field of linear type `{}`", self.prog.short_name(d.name), self.show(&fcx, &a)))
                                        .with_help("declare the data type in `LType`"),
                                );
                            }
                        }
                        mults.push(m);
                        icits.push(i);
                        let l = self.push(&mut fcx, &x, m, &a, None, c.span, None);
                        v = self.ev().inst(&clo, Val::var(l));
                    }
                    Val::Data(g, args) if g == d.name => {
                        for (k, (a, _)) in args.iter().take(nparams).enumerate() {
                            if !matches!(self.whnf(a), Val::Neutral(Head::Var(l), ref s) if l == k as u32 && s.is_empty())
                            {
                                self.report(self.error(
                                    "E1201",
                                    c.span,
                                    format!("constructor `{}` must return `{}` applied to its parameters", self.prog.short_name(c.name), self.prog.short_name(d.name)),
                                ));
                                break;
                            }
                        }
                        break;
                    }
                    Val::Error => break,
                    other => {
                        self.report(
                            self.error(
                                "E1201",
                                c.span,
                                format!(
                                    "constructor `{}` must return `{}`",
                                    self.prog.short_name(c.name),
                                    self.prog.short_name(d.name)
                                ),
                            )
                            .with_note(format!("it returns `{}`", self.show(&fcx, &other))),
                        );
                        break;
                    }
                }
            }
            let mut ty = sig;
            for (x, a) in ps.into_iter().rev() {
                ty = Term::Pi(x, Mult::Zero, Icit::Impl, Rc::new(a), Rc::new(ty));
            }
            self.prog.globals[c.name.0 as usize] = Global::Con {
                ty,
                data: d.name,
                nparams,
                field_mults: mults,
                field_icits: icits,
                tag,
            };
            self.prog.order.push(c.name);
        }
        let univ = if d.short && lin_field {
            Univ::LType
        } else {
            univ
        };
        if let Global::Data { univ: u, .. } = &mut self.prog.globals[d.name.0 as usize] {
            *u = univ;
        }
    }

    fn prim(&mut self, g: GName, t: &rast::Expr) {
        self.begin(self.prog.name(g).to_string());
        let mut cx = Cxt::default();
        let tt = self.check_type(&mut cx, t);
        self.solve_pending(true);
        let ty = self.zonk(0, &tt);
        let name = &self.rp.info(g).name;
        match Prim::from_name(name) {
            Some(p) => self.prog.globals[g.0 as usize] = Global::Prim { ty, prim: p },
            None => self.report(self.error(
                "E1201",
                self.rp.info(g).span,
                format!("unknown primitive `{name}`"),
            )),
        }
        self.prog.order.push(g);
    }

    fn def_sig(&mut self, d: &rast::Def) {
        self.begin(self.prog.name(d.name).to_string());
        let mut cx = Cxt::default();
        let tt = self.check_type(&mut cx, &d.sig);
        self.solve_pending(true);
        let ty = self.zonk(0, &tt);
        let short = self.prog.short_name(d.name).to_string();
        self.check_shared_type(
            &ty,
            d.sig.span,
            &format!("`{short}`"),
            &short,
            "a top-level value is shared by all its uses",
        );
        self.prog.globals[d.name.0 as usize] = Global::Def { ty, body: None };
    }

    fn def_body(&mut self, d: &rast::Def) {
        let name = self.prog.name(d.name).to_string();
        self.begin(name.clone());
        let Global::Def { ty, .. } = self.prog.get(d.name).clone() else {
            return;
        };
        let before = self.error_count();
        let tv = self.ev().eval(&Vec::new(), &ty);
        let mut cx = Cxt::default();
        let body = self.elab_clauses(&mut cx, &name, &d.clauses, &tv, d.span);
        let body = self.finish(&cx, &body);
        let body = self.share_top(&mut cx, &ty, body);
        let body = if self.error_count() == before {
            self.insert_finalizers(&ty, body, d.span)
        } else {
            body
        };
        let after = self.error_count();
        self.prog.globals[d.name.0 as usize] = Global::Def {
            ty,
            body: if after == before { Some(body) } else { None },
        };
        self.prog.order.push(d.name);
    }
}

/// Whether the leading lambdas of `t` take a runtime argument (an erased
/// lambda disappears at runtime: its body is evaluated with the value
/// around it).
fn runtime_lam(t: &Term) -> bool {
    match t {
        Term::Lam(_, m, _, _, b) => *m != Mult::Zero || runtime_lam(b),
        _ => false,
    }
}

fn conv_mult(m: rast::Mult) -> Mult {
    match m {
        rast::Mult::Zero => Mult::Zero,
        rast::Mult::One => Mult::One,
        rast::Mult::Many => Mult::Many,
    }
}

/// Rewrites integer literal patterns into `S (S … Z)`.
fn nat_pat(b: &Builtins, q: &mut rast::Pat) {
    match &mut q.kind {
        PatKind::Lit(SLit::Int(n), false) if *n <= 10_000 => {
            if let (Some(z), Some(s)) = (b.zero, b.succ) {
                let mut p = rast::Pat {
                    kind: PatKind::Con(z, Vec::new()),
                    span: q.span,
                };
                for _ in 0..*n {
                    p = rast::Pat {
                        kind: PatKind::Con(s, vec![p]),
                        span: q.span,
                    };
                }
                *q = p;
            }
        }
        PatKind::Con(_, ps) => {
            for p in ps {
                nat_pat(b, p);
            }
        }
        _ => {}
    }
}
