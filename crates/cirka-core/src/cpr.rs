//! Constructed product results (FEAT-0065, 06-ir.md §1.7, ADR-049).
//!
//! A top-level function whose every return builds the single constructor
//! of a product type (a tuple, `Ur`, a record) is split into a worker that
//! returns the fields as an *unboxed tuple* `UTup<n>` and a wrapper that
//! rebuilds the value:
//!
//! ```text
//! f    = λx̄. … (Tuple2 u (Ur (I# n))) … f ȳ …
//! ==>
//! f$c  = λx̄. … (UTup2 u n) … f$c ȳ …
//! f    = λx̄. case f$c x̄ of UTup2 a b -> Tuple2 a (Ur (I# b))
//! ```
//!
//! The wrapper is inlined at the call sites, where the rebuilt constructor
//! meets the `case` that takes it apart: nothing is allocated. Fields that
//! every return builds with a known constructor are flattened (the `Ur` and
//! `I#` above). The shapes of mutually recursive functions are a greatest
//! fixed point: a tail call of another such function contributes its shape.
//!
//! A function whose every return builds a constructor of one *sum* type
//! (FEAT-1008), with fields that flatten to unboxed values only, returns
//! the index of the constructor and the leaves of every constructor it
//! builds, zeros for those of the others:
//!
//! ```text
//! f    = λx̄. … Nothing … (Just (I# n)) … f ȳ …
//! ==>
//! f$c  = λx̄. … (UTup2 0 0) … (UTup2 1 n) … f$c ȳ …
//! f    = λx̄. case f$c x̄ of UTup2 t a -> case t of 0 -> Nothing; _ -> Just (I# a)
//! ```
//!
//! At a call site that takes the result apart, case-of-case meets the
//! constructors of the inlined wrapper: `toInt` (a loop ending in `Just`
//! or `Nothing`) allocates neither the `Just` nor the `I#`. The leaves of
//! a sum must be cheap in every return (atoms or primitive operations on
//! them): returning them evaluates nothing a lazy field would have left
//! unevaluated.
//!
//! `UTup<n>` has no runtime representation. Core Lint ([`check_placement`])
//! only allows it to be built in the return position of a top-level
//! function body and taken apart by a `case` on a saturated call; MIR
//! returns its fields as the function's multiple results. A `case` may
//! also take apart the tuples that branches return (a join,
//! [`crate::join`], FEAT-1009): MIR passes them to a join block.

use crate::expr::*;
use crate::opt::walk;
use crate::ty::*;
use crate::ww::split_fun;
use std::collections::{HashMap, HashSet};

/// Constructors nested deeper than this are not flattened.
const MAX_DEPTH: usize = 3;
/// At most this many results.
const MAX_LEAVES: usize = 8;

/// The name of the unboxed tuple type (and constructor) with `n` fields.
pub fn utup_name(n: usize) -> String {
    format!("UTup{n}")
}

/// Whether `c` names an unboxed tuple type or constructor.
pub fn is_utup(c: &str) -> bool {
    c.strip_prefix("UTup")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Declares `UTup<n>` in `p` (once).
pub(crate) fn ensure_utup(p: &mut Program, n: usize) -> String {
    let name = utup_name(n);
    if p.data(&name).is_none() {
        let params: Vec<(TyVar, Kind)> = (0..n)
            .map(|i| (TyVar::fresh(&format!("t{i}")), Kind::Type))
            .collect();
        let fields = params.iter().map(|(v, _)| Type::Var(v.clone())).collect();
        p.datas.push(DataDecl {
            name: name.clone(),
            params,
            linear: false,
            ctors: vec![CtorDecl {
                name: name.clone(),
                fields,
                strict: vec![false; n],
            }],
            newtype: false,
        });
    }
    name
}

/// The result type of a top-level function type after its arguments.
fn result_of(t: &Type) -> &Type {
    let mut t = t;
    loop {
        match t {
            Type::Forall(_, _, b) | Type::Fun(_, _, b) => t = b,
            _ => return t,
        }
    }
}

/// Whether a function of type `t` returns an unboxed tuple.
pub(crate) fn returns_utup(t: &Type) -> bool {
    matches!(result_of(t), Type::Con(c, _) if is_utup(c))
}

/// The globals returning an unboxed tuple.
pub(crate) fn utup_functions(p: &Program) -> HashSet<String> {
    p.binds
        .iter()
        .filter(|b| matches!(result_of(&b.ty), Type::Con(c, _) if is_utup(c)))
        .map(|b| b.name.clone())
        .collect()
}

/// Core Lint's placement rule for unboxed tuples (06-ir.md §1.7), in a
/// program that declares them: `fs` are its [`utup_functions`].
pub(crate) fn check_placement(fs: &HashSet<String>, b: &TopBind) -> Result<(), String> {
    let (_, _, body) = split_fun(&b.expr);
    place(body, true, fs)
}

fn utup_type(t: &Type) -> bool {
    matches!(t, Type::Con(c, _) if is_utup(c))
}

fn call_head(e: &Expr) -> Option<&str> {
    let (mut h, _) = e.collect_args();
    while let Expr::TyApp(x, _) = h {
        h = x;
    }
    match h {
        Expr::Global(g) => Some(g),
        _ => None,
    }
}

/// A binder of an unboxed tuple: never allowed (a tuple is not a value).
fn no_tuple(b: &Binder) -> Result<(), String> {
    if utup_type(&b.ty) {
        Err(format!("`{}` binds an unboxed tuple", b.id.name))
    } else {
        Ok(())
    }
}

/// The scrutinee `s` of a case with binder `cb` and alternatives `alts`.
/// When the case takes an unboxed tuple apart, `s` builds one (see
/// [`tuple`]) and the binder is not used.
fn scrutinee(s: &Expr, cb: &Binder, alts: &[Alt], fs: &HashSet<String>) -> Result<(), String> {
    let takes_apart = alts
        .iter()
        .any(|a| matches!(&a.con, AltCon::Con(c, _) if is_utup(c)));
    if !(takes_apart || utup_type(&cb.ty)) {
        return place(s, false, fs);
    }
    tuple(s, fs)?;
    let mut used = false;
    for a in alts {
        walk(&a.rhs, &mut |x| {
            used |= matches!(x, Expr::Var(v) if v.id == cb.id.id);
        });
    }
    if used {
        return Err("the binder of an unboxed tuple is used".into());
    }
    Ok(())
}

/// An unboxed tuple that a case takes apart, after bindings (strictify
/// evaluates arguments first): a saturated call of a function returning
/// one, or branches whose returns are such tuples (a join, FEAT-1009):
/// the tuple itself, such a call, a panic, or more branches.
fn tuple(e: &Expr, fs: &HashSet<String>) -> Result<(), String> {
    let mut e = e;
    while let Expr::Let(Bind::NonRec(b, r), body) | Expr::LetStrict(b, r, body) = e {
        no_tuple(b)?;
        place(r, false, fs)?;
        e = body;
    }
    match e {
        Expr::Con(c, _, args) if is_utup(c) => args.iter().try_for_each(|a| place(a, false, fs)),
        Expr::Panic(..) => Ok(()),
        Expr::Case(s, cb, _, alts) => {
            scrutinee(s, cb, alts, fs)?;
            alts.iter().try_for_each(|a| {
                if let AltCon::Con(_, bs) = &a.con {
                    bs.iter().try_for_each(no_tuple)?;
                }
                tuple(&a.rhs, fs)
            })
        }
        _ if call_head(e).is_some_and(|g| fs.contains(g)) => {
            let (_, args) = e.collect_args();
            args.iter().try_for_each(|a| place(a, false, fs))
        }
        _ => Err("an unboxed tuple taken apart from something else than a call or branches".into()),
    }
}

fn place(e: &Expr, tail: bool, fs: &HashSet<String>) -> Result<(), String> {
    let no = no_tuple;
    match e {
        Expr::Con(c, _, args) => {
            if is_utup(c) && !tail {
                return Err(format!("`{c}` built outside a return position"));
            }
            args.iter().try_for_each(|a| place(a, false, fs))
        }
        Expr::Case(s, cb, _, alts) => {
            scrutinee(s, cb, alts, fs)?;
            alts.iter().try_for_each(|a| {
                if let AltCon::Con(_, bs) = &a.con {
                    bs.iter().try_for_each(no)?;
                }
                place(&a.rhs, tail, fs)
            })
        }
        Expr::Let(Bind::NonRec(b, r), body) | Expr::LetStrict(b, r, body) => {
            no(b)?;
            place(r, false, fs)?;
            place(body, tail, fs)
        }
        Expr::Let(Bind::Rec(bs), body) => {
            for (b, r) in bs {
                no(b)?;
                place(r, false, fs)?;
            }
            place(body, tail, fs)
        }
        Expr::Lam(b, body) => {
            no(b)?;
            place(body, false, fs)
        }
        Expr::App(..) => {
            if !tail && call_head(e).is_some_and(|g| fs.contains(g)) {
                return Err(format!(
                    "the unboxed tuple returned by `{}` is not taken apart",
                    call_head(e).unwrap_or("")
                ));
            }
            let (h, args) = e.collect_args();
            if call_head(e).is_none() {
                place(h, false, fs)?;
            }
            args.iter().try_for_each(|a| place(a, false, fs))
        }
        Expr::Global(g) => {
            if fs.contains(g.as_str()) && !tail {
                return Err(format!(
                    "`{g}` (returning an unboxed tuple) used as a value"
                ));
            }
            Ok(())
        }
        Expr::TyApp(x, _) => place(x, tail, fs),
        Expr::TyLam(_, _, x) | Expr::Cast(x, _) => place(x, false, fs),
        Expr::Prim(_, _, a) | Expr::Extern(_, a) => a.iter().try_for_each(|x| place(x, false, fs)),
        Expr::Var(_) | Expr::Lit(_) | Expr::Panic(..) => Ok(()),
    }
}

// ---- shapes ----

/// What every return of a function builds.
#[derive(Clone, Debug)]
enum Shape {
    /// Returned as is.
    Leaf(Type),
    /// The constructor `k` (of type `ty`, type arguments `ts`) with these
    /// fields.
    Con {
        k: String,
        ts: Vec<Type>,
        ty: Type,
        fields: Vec<Shape>,
    },
    /// One of the constructors of the sum type `ty` (type arguments `ts`),
    /// in declaration order, with the shapes of its fields, or `None` for
    /// one that no return builds (FEAT-1008). Only at the top. Its results
    /// are the index of the constructor (`Int#`), then the leaves of each
    /// constructor built, in order.
    Sum {
        ts: Vec<Type>,
        ty: Type,
        alts: Vec<(String, Option<Vec<Shape>>)>,
    },
}

impl Shape {
    fn ty(&self) -> &Type {
        match self {
            Shape::Leaf(t) | Shape::Con { ty: t, .. } | Shape::Sum { ty: t, .. } => t,
        }
    }

    fn leaves(&self, out: &mut Vec<Type>) {
        match self {
            Shape::Leaf(t) => out.push(t.clone()),
            Shape::Con { fields, .. } => fields.iter().for_each(|f| f.leaves(out)),
            Shape::Sum { alts, .. } => {
                out.push(Type::int());
                for f in alts.iter().filter_map(|(_, fs)| fs.as_ref()).flatten() {
                    f.leaves(out);
                }
            }
        }
    }

    fn same(&self, o: &Shape) -> bool {
        match (self, o) {
            (Shape::Leaf(a), Shape::Leaf(b)) => a.alpha_eq(b),
            (
                Shape::Con {
                    k: k1, fields: f1, ..
                },
                Shape::Con {
                    k: k2, fields: f2, ..
                },
            ) => k1 == k2 && same_fields(f1, f2),
            (Shape::Sum { alts: a1, .. }, Shape::Sum { alts: a2, .. }) => {
                a1.len() == a2.len()
                    && a1.iter().zip(a2).all(|((k1, f1), (k2, f2))| {
                        k1 == k2
                            && match (f1, f2) {
                                (None, None) => true,
                                (Some(x), Some(y)) => same_fields(x, y),
                                _ => false,
                            }
                    })
            }
            _ => false,
        }
    }

    /// The most precise shape below both.
    fn meet(self, o: &Shape) -> Shape {
        match (self, o) {
            (
                Shape::Con { k, ts, ty, fields },
                Shape::Con {
                    k: k2, fields: f2, ..
                },
            ) if &k == k2 && fields.len() == f2.len() => Shape::Con {
                k,
                ts,
                ty,
                fields: fields.into_iter().zip(f2).map(|(a, b)| a.meet(b)).collect(),
            },
            // The constructors either builds, with the fields met where
            // both build one.
            (Shape::Sum { ts, ty, alts }, Shape::Sum { alts: a2, .. })
                if alts.len() == a2.len()
                    && alts.iter().zip(a2).all(|((k1, f1), (k2, f2))| {
                        k1 == k2
                            && match (f1, f2) {
                                (Some(x), Some(y)) => x.len() == y.len(),
                                _ => true,
                            }
                    }) =>
            {
                Shape::Sum {
                    ts,
                    ty,
                    alts: alts
                        .into_iter()
                        .zip(a2)
                        .map(|((k, f1), (_, f2))| {
                            let f = match (f1, f2) {
                                (None, f2) => f2.clone(),
                                (f1, None) => f1,
                                (Some(x), Some(y)) => {
                                    Some(x.into_iter().zip(y).map(|(a, b)| a.meet(b)).collect())
                                }
                            };
                            (k, f)
                        })
                        .collect(),
                }
            }
            (s, _) => Shape::Leaf(s.ty().clone()),
        }
    }
}

fn same_fields(f1: &[Shape], f2: &[Shape]) -> bool {
    f1.len() == f2.len() && f1.iter().zip(f2).all(|(a, b)| a.same(b))
}

/// The number of leaves of the fields `fs`.
fn field_leaves(fs: &[Shape]) -> Vec<Type> {
    let mut out = Vec::new();
    fs.iter().for_each(|f| f.leaves(&mut out));
    out
}

/// A value of the unboxed type `t`: the leaves of the constructors a
/// return of a sum does not build.
fn zero(t: &Type) -> Expr {
    match t {
        Type::Con(c, _) if c == UNIT => Expr::Lit(Lit::Unit),
        Type::Con(c, _) if c == DOUBLE => Expr::Lit(Lit::Double(0.0)),
        _ => Expr::Lit(Lit::Int(0)),
    }
}

/// The single constructor of the product type `t`, its type arguments and
/// field types.
fn product(p: &Program, t: &Type) -> Option<(String, Vec<Type>, Vec<Type>)> {
    let Type::Con(d, args) = t else { return None };
    if is_utup(d) {
        return None;
    }
    let d = p.data(d)?;
    if d.newtype || d.linear || d.ctors.len() != 1 || d.params.len() != args.len() {
        return None;
    }
    let s: Vec<(TyVar, Type)> = d
        .params
        .iter()
        .map(|x| x.0.clone())
        .zip(args.iter().cloned())
        .collect();
    let k = &d.ctors[0];
    Some((
        k.name.clone(),
        args.clone(),
        k.fields.iter().map(|f| f.subst_all(&s)).collect(),
    ))
}

/// The constructors (two or more) of the sum type `t`, its type arguments
/// and the field types of each constructor.
type SumType = (Vec<Type>, Vec<(String, Vec<Type>)>);

fn sum_type(p: &Program, t: &Type) -> Option<SumType> {
    let Type::Con(d, args) = t else { return None };
    if is_utup(d) {
        return None;
    }
    let d = p.data(d)?;
    if d.newtype || d.linear || d.ctors.len() < 2 || d.params.len() != args.len() {
        return None;
    }
    let s: Vec<(TyVar, Type)> = d
        .params
        .iter()
        .map(|x| x.0.clone())
        .zip(args.iter().cloned())
        .collect();
    Some((
        args.clone(),
        d.ctors
            .iter()
            .map(|k| {
                (
                    k.name.clone(),
                    k.fields.iter().map(|f| f.subst_all(&s)).collect(),
                )
            })
            .collect(),
    ))
}

/// Variables bound (with multiplicity ω) to a constructor application.
type Env = HashMap<u32, Expr>;

fn resolve<'a>(e: &'a Expr, env: &'a Env) -> &'a Expr {
    match e {
        Expr::Var(v) => env.get(&v.id).unwrap_or(e),
        e => e,
    }
}

/// Evaluating `e` terminates without effects and costs little: an atom,
/// or a primitive operation on such.
fn cheap(e: &Expr) -> bool {
    match e {
        Expr::Var(_) | Expr::Lit(_) => true,
        Expr::Prim(op, _, a) => !op.effectful() && a.iter().all(cheap),
        _ => false,
    }
}

/// Whether the leaves of `e` (of shape `s`) are [`cheap`]: returning them
/// evaluates nothing that a lazy field would have left unevaluated.
fn cheap_leaves(s: &Shape, e: &Expr, env: &Env) -> bool {
    match s {
        Shape::Leaf(_) => cheap(e),
        Shape::Con { fields, .. } => match resolve(e, env) {
            Expr::Con(_, _, args) => fields
                .iter()
                .zip(args)
                .all(|(f, a)| cheap_leaves(f, a, env)),
            _ => false,
        },
        Shape::Sum { .. } => false,
    }
}

/// The shape of the value `e` of type `t`.
fn shape_of(p: &Program, e: &Expr, env: &Env, t: &Type, depth: usize) -> Shape {
    // A constructor of a sum type (FEAT-1008), at the top only, with
    // cheap leaves.
    if depth == 0
        && let Some((ts, ctors)) = sum_type(p, t)
        && let Expr::Con(c, _, args) = resolve(e, env)
        && let Some(i) = ctors.iter().position(|(k, _)| k == c)
        && args.len() == ctors[i].1.len()
    {
        let fields: Vec<Shape> = args
            .iter()
            .zip(&ctors[i].1)
            .map(|(a, ft)| shape_of(p, a, env, ft, depth + 1))
            .collect();
        if !fields
            .iter()
            .zip(args)
            .all(|(f, a)| cheap_leaves(f, a, env))
        {
            return Shape::Leaf(t.clone());
        }
        let mut fields = Some(fields);
        return Shape::Sum {
            ts,
            ty: t.clone(),
            alts: ctors
                .into_iter()
                .enumerate()
                .map(|(j, (k, _))| (k, if j == i { fields.take() } else { None }))
                .collect(),
        };
    }
    if depth < MAX_DEPTH
        && let Some((k, ts, fts)) = product(p, t)
        && let Expr::Con(c, _, args) = resolve(e, env)
        && *c == k
        && args.len() == fts.len()
    {
        return Shape::Con {
            k,
            ts,
            ty: t.clone(),
            fields: args
                .iter()
                .zip(&fts)
                .map(|(a, ft)| shape_of(p, a, env, ft, depth + 1))
                .collect(),
        };
    }
    Shape::Leaf(t.clone())
}

/// A function that may return a product.
struct Cand {
    tvs: Vec<(TyVar, Kind)>,
    params: Vec<Binder>,
    res: Type,
}

/// A saturated call `g @ts args` of a candidate in return position, made
/// in `me`. A polymorphic `g` only when it is `me` at its own type
/// variables (the shapes are in terms of them).
fn cand_call<'a>(e: &'a Expr, me: &str, cands: &HashMap<String, Cand>) -> Option<&'a str> {
    let g = call_head(e)?;
    let c = cands.get(g)?;
    let (mut h, args) = e.collect_args();
    if args.len() != c.params.len() {
        return None;
    }
    let mut ts = Vec::new();
    while let Expr::TyApp(x, t) = h {
        ts.push(t);
        h = x;
    }
    ts.reverse();
    let own = g == me
        && ts.len() == c.tvs.len()
        && ts
            .iter()
            .zip(&c.tvs)
            .all(|(t, (v, _))| matches!(t, Type::Var(w) if w == v));
    (c.tvs.is_empty() || own).then_some(g)
}

/// Whether `x` is a call of a candidate whose result type is `t`.
fn same_result_call(x: &Expr, t: &Type, me: &str, cands: &HashMap<String, Cand>) -> bool {
    cand_call(x, me, cands).is_some_and(|g| cands[g].tvs.is_empty() && cands[g].res.alpha_eq(t))
}

/// Meets the shapes of the returns of `e` into `acc`.
#[allow(clippy::too_many_arguments)]
fn returns(
    p: &Program,
    me: &str,
    e: &Expr,
    env: &mut Env,
    res: &Type,
    cands: &HashMap<String, Cand>,
    shapes: &HashMap<String, Option<Shape>>,
    acc: &mut Option<Shape>,
) {
    let meet = |s: Shape, acc: &mut Option<Shape>| {
        *acc = Some(match acc.take() {
            None => s,
            Some(a) => a.meet(&s),
        });
    };
    match e {
        Expr::Let(Bind::NonRec(b, r), body) | Expr::LetStrict(b, r, body) => {
            if b.mult == Mult::Many && matches!(&**r, Expr::Con(..)) {
                env.insert(b.id.id, (**r).clone());
            }
            returns(p, me, body, env, res, cands, shapes, acc);
        }
        Expr::Let(Bind::Rec(_), body) => returns(p, me, body, env, res, cands, shapes, acc),
        Expr::Case(_, _, _, alts) => {
            for a in alts {
                returns(p, me, &a.rhs, env, res, cands, shapes, acc);
            }
        }
        Expr::Panic(..) => {}
        // A call cast to its own result type (left by the state-token
        // workers, `io.rs`) is the call.
        Expr::Cast(x, t) if same_result_call(x, t, me, cands) => {
            returns(p, me, x, env, res, cands, shapes, acc)
        }
        e => match cand_call(e, me, cands) {
            Some(g) => {
                if let Some(Some(s)) = shapes.get(g) {
                    meet(s.clone(), acc);
                }
            }
            None => meet(shape_of(p, e, env, res, 0), acc),
        },
    }
}

/// Whether a shape is worth a worker: a constructor with a few results,
/// and a single result only when it is unboxed (a lazy field may be a
/// thunk, which a function must not return).
///
/// A sum (FEAT-1008): unboxed leaves only (those of the constructors a
/// return does not build are zeros), at least one besides the index.
fn acceptable(s: &Shape) -> bool {
    let mut ls = Vec::new();
    s.leaves(&mut ls);
    match s {
        Shape::Leaf(_) => false,
        Shape::Con { .. } => match ls.len() {
            0 => false,
            1 => ls[0].is_unboxed(),
            n => n <= MAX_LEAVES,
        },
        Shape::Sum { .. } => {
            ls.len() > 1 && ls.len() <= MAX_LEAVES && ls.iter().all(Type::is_unboxed)
        }
    }
}

/// Splits the functions with constructed product results; returns how many.
pub fn product_results(p: &mut Program, next: &mut u32) -> usize {
    let mut cands = HashMap::new();
    for b in &p.binds {
        // Workers already split, and wrappers (they only call a worker).
        if b.name == "main" || b.name.ends_with("$c") || p.bind(&format!("{}$w", b.name)).is_some()
        {
            continue;
        }
        let (tvs, xs, _) = split_fun(&b.expr);
        if xs.is_empty() {
            continue;
        }
        // The result type in terms of the `Λ`-bound variables.
        let mut ty = b.ty.clone();
        let mut k = 0;
        while let Type::Forall(v, _, t) = ty {
            ty = match tvs.get(k) {
                Some((w, _)) => t.subst(&v, &Type::Var(w.clone())),
                None => *t,
            };
            k += 1;
        }
        if k != tvs.len() {
            continue;
        }
        let mut res = ty;
        let mut ok = true;
        for _ in 0..xs.len() {
            match res {
                Type::Fun(_, _, r) => res = *r,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || (product(p, &res).is_none() && sum_type(p, &res).is_none()) {
            continue;
        }
        cands.insert(
            b.name.clone(),
            Cand {
                tvs,
                params: xs,
                res,
            },
        );
    }
    if cands.is_empty() {
        return 0;
    }
    // Greatest fixed point: start from "anything" (None) and weaken.
    let mut shapes: HashMap<String, Option<Shape>> =
        cands.keys().map(|k| (k.clone(), None)).collect();
    loop {
        let mut changed = false;
        for b in &p.binds {
            let Some(c) = cands.get(&b.name) else {
                continue;
            };
            let (_, _, body) = split_fun(&b.expr);
            let mut acc = None;
            returns(
                p,
                &b.name,
                body,
                &mut Env::new(),
                &c.res,
                &cands,
                &shapes,
                &mut acc,
            );
            let new = match acc {
                Some(s) if acceptable(&s) => Some(s),
                Some(_) => Some(Shape::Leaf(c.res.clone())),
                // Only divergence and calls of functions not known yet.
                None => None,
            };
            let old = &shapes[&b.name];
            let same = match (old, &new) {
                (None, None) => true,
                (Some(a), Some(b)) => a.same(b),
                _ => false,
            };
            if !same {
                shapes.insert(b.name.clone(), new);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let plans: HashMap<String, Shape> = shapes
        .into_iter()
        .filter_map(|(f, s)| s.filter(acceptable).map(|s| (f, s)))
        .collect();
    if plans.is_empty() {
        return 0;
    }
    let mut workers = HashMap::new();
    let names: HashSet<String> = p.binds.iter().map(|b| b.name.clone()).collect();
    for f in plans.keys() {
        let mut w = format!("{f}$c");
        let mut i = 1;
        while names.contains(&w) {
            i += 1;
            w = format!("{f}$c{i}");
        }
        workers.insert(f.clone(), w);
    }
    let mut n = 0;
    let mut i = 0;
    while i < p.binds.len() {
        let name = p.binds[i].name.clone();
        let Some(shape) = plans.get(&name) else {
            i += 1;
            continue;
        };
        let c = &cands[&name];
        let mut lts = Vec::new();
        shape.leaves(&mut lts);
        let wres = if lts.len() == 1 {
            lts[0].clone()
        } else {
            let u = ensure_utup(p, lts.len());
            Type::Con(u, lts.clone())
        };
        let (_, _, body) = split_fun(&p.binds[i].expr);
        let body = body.clone();
        let t = Tr {
            p,
            plans: &plans,
            workers: &workers,
            cands: &cands,
            me: &name,
            shape,
            wres: &wres,
        };
        let wbody = t.expr(body, &mut Env::new(), next);
        let worker = workers[&name].clone();
        let wty = fun_type(&c.tvs, &c.params, &wres);
        let wexpr = abstract_fun(&c.tvs, &c.params, wbody);
        // Wrapper: call the worker and rebuild the value.
        let ps: Vec<Binder> = c
            .params
            .iter()
            .map(|x| {
                let b = Binder::new(Id::new(x.id.name.clone(), *next), x.mult, x.ty.clone());
                *next += 1;
                b
            })
            .collect();
        let mut call = Expr::Global(worker.clone());
        for (v, _) in &c.tvs {
            call = Expr::TyApp(Box::new(call), Type::Var(v.clone()));
        }
        let call = Expr::app(call, ps.iter().map(|b| Expr::var(&b.id)));
        let lbs: Vec<Binder> = lts
            .iter()
            .map(|t| {
                let m = if linear(p, t, &c.tvs, 0) {
                    Mult::One
                } else {
                    Mult::Many
                };
                let b = Binder::new(Id::new("r", *next), m, t.clone());
                *next += 1;
                b
            })
            .collect();
        let rebuilt = match shape {
            Shape::Sum { .. } => rebuild_sum(p, &c.tvs, shape, &lbs, &c.res, next),
            _ => {
                let mut it = lbs.iter().map(|b| Expr::var(&b.id));
                build_bound(p, &c.tvs, shape, &mut it, next)
            }
        };
        let wrap = if lbs.len() == 1 {
            Expr::LetStrict(lbs[0].clone(), Box::new(call), Box::new(rebuilt))
        } else {
            let cb = Binder::new(Id::new("u", *next), Mult::Many, wres.clone());
            *next += 1;
            Expr::Case(
                Box::new(call),
                cb,
                c.res.clone(),
                vec![Alt {
                    con: AltCon::Con(utup_name(lbs.len()), lbs),
                    rhs: rebuilt,
                }],
            )
        };
        let span = p.binds[i].span;
        p.binds[i].expr = abstract_fun(&c.tvs, &ps, wrap);
        p.binds.insert(
            i + 1,
            TopBind {
                name: worker,
                ty: wty,
                expr: wexpr,
                span,
            },
        );
        n += 1;
        i += 2;
    }
    n
}

/// Whether values of type `t` are linear (kind `LType`).
fn linear(p: &Program, t: &Type, tvs: &[(TyVar, Kind)], depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    match t {
        Type::Var(v) => tvs.iter().any(|(w, k)| w == v && *k == Kind::LType),
        Type::Con(c, args) => {
            let Some(d) = p.data(c) else { return false };
            if d.linear {
                return true;
            }
            let s: Vec<(TyVar, Type)> = d
                .params
                .iter()
                .map(|x| x.0.clone())
                .zip(args.iter().cloned())
                .collect();
            d.ctors
                .iter()
                .flat_map(|k| &k.fields)
                .any(|f| linear(p, &f.subst_all(&s), tvs, depth + 1))
        }
        _ => false,
    }
}

/// The value of shape `s` rebuilt from its leaves, with every inner
/// constructor bound by a `let!` first: each constructor is applied to
/// atoms, so the simplifier knows it where the wrapper is inlined.
fn build_bound(
    p: &Program,
    tvs: &[(TyVar, Kind)],
    s: &Shape,
    leaves: &mut impl Iterator<Item = Expr>,
    next: &mut u32,
) -> Expr {
    let mut binds = Vec::new();
    let e = atoms(p, tvs, s, leaves, next, &mut binds, true);
    binds.into_iter().rev().fold(e, |body, (b, r)| {
        Expr::LetStrict(b, Box::new(r), Box::new(body))
    })
}

fn atoms(
    p: &Program,
    tvs: &[(TyVar, Kind)],
    s: &Shape,
    leaves: &mut impl Iterator<Item = Expr>,
    next: &mut u32,
    binds: &mut Vec<(Binder, Expr)>,
    top: bool,
) -> Expr {
    match s {
        Shape::Leaf(_) => leaves.next().expect("one leaf per result"),
        Shape::Con { k, ts, ty, fields } => {
            let args = fields
                .iter()
                .map(|f| atoms(p, tvs, f, leaves, next, binds, false))
                .collect();
            let e = Expr::Con(k.clone(), ts.clone(), args);
            if top {
                return e;
            }
            let m = if linear(p, ty, tvs, 0) {
                Mult::One
            } else {
                Mult::Many
            };
            let b = Binder::new(Id::new("v", *next), m, ty.clone());
            *next += 1;
            binds.push((b.clone(), e));
            Expr::var(&b.id)
        }
        Shape::Sum { .. } => unreachable!("a sum is rebuilt by `rebuild_sum`"),
    }
}

/// The value of the sum shape `s` (of type `res`) rebuilt from the binders
/// of its leaves: a case on the index of the constructor (the first leaf)
/// that builds the constructor from its own leaves.
fn rebuild_sum(
    p: &Program,
    tvs: &[(TyVar, Kind)],
    s: &Shape,
    lbs: &[Binder],
    res: &Type,
    next: &mut u32,
) -> Expr {
    let Shape::Sum { ts, alts, .. } = s else {
        unreachable!("a sum shape")
    };
    let mut rest = lbs[1..].iter();
    let mut built = Vec::new();
    for (i, (k, fs)) in alts.iter().enumerate() {
        let Some(fs) = fs else { continue };
        let n = field_leaves(fs).len();
        let mut mine = rest.by_ref().take(n).map(|b| Expr::var(&b.id));
        let con = Shape::Con {
            k: k.clone(),
            ts: ts.clone(),
            ty: res.clone(),
            fields: fs.clone(),
        };
        built.push((i, build_bound(p, tvs, &con, &mut mine, next)));
    }
    let last = built.len() - 1;
    let alts = built
        .into_iter()
        .enumerate()
        .map(|(n, (i, rhs))| Alt {
            con: if n == last {
                AltCon::Default
            } else {
                AltCon::Int(i as i64)
            },
            rhs,
        })
        .collect();
    let tb = Binder::new(Id::new("t", *next), Mult::Many, Type::int());
    *next += 1;
    Expr::Case(Box::new(Expr::var(&lbs[0].id)), tb, res.clone(), alts)
}

/// The value of shape `s` rebuilt from its leaves.
fn build(s: &Shape, leaves: &mut impl Iterator<Item = Expr>) -> Expr {
    match s {
        Shape::Leaf(_) => leaves.next().expect("one leaf per result"),
        Shape::Con { k, ts, fields, .. } => Expr::Con(
            k.clone(),
            ts.clone(),
            fields.iter().map(|f| build(f, leaves)).collect(),
        ),
        Shape::Sum { .. } => unreachable!("a sum is only at the top"),
    }
}

/// The leaves of the value `e` (of shape at least `s`).
fn leaves_of(s: &Shape, e: Expr, env: &Env, out: &mut Vec<Expr>) {
    match s {
        Shape::Leaf(_) => out.push(e),
        Shape::Sum { alts, .. } => {
            let e = match e {
                Expr::Var(v) if env.contains_key(&v.id) => env[&v.id].clone(),
                e => e,
            };
            let Expr::Con(c, _, args) = e else {
                unreachable!("shape analysis found a constructor here")
            };
            let i = alts
                .iter()
                .position(|(k, _)| *k == c)
                .expect("a constructor of the shape");
            out.push(Expr::Lit(Lit::Int(i as i64)));
            let mut args = Some(args);
            for (j, (_, fs)) in alts.iter().enumerate() {
                let Some(fs) = fs else { continue };
                if j == i {
                    for (f, a) in fs.iter().zip(args.take().unwrap_or_default()) {
                        leaves_of(f, a, env, out);
                    }
                } else {
                    out.extend(field_leaves(fs).iter().map(zero));
                }
            }
        }
        Shape::Con { fields, .. } => {
            let e = match e {
                Expr::Var(v) if env.contains_key(&v.id) => env[&v.id].clone(),
                e => e,
            };
            let Expr::Con(_, _, args) = e else {
                unreachable!("shape analysis found a constructor here")
            };
            for (f, a) in fields.iter().zip(args) {
                leaves_of(f, a, env, out);
            }
        }
    }
}

/// Rewrites the returns of a worker body.
struct Tr<'a> {
    p: &'a Program,
    plans: &'a HashMap<String, Shape>,
    workers: &'a HashMap<String, String>,
    cands: &'a HashMap<String, Cand>,
    me: &'a str,
    shape: &'a Shape,
    wres: &'a Type,
}

impl Tr<'_> {
    /// The results of the call of another sum worker (shape `gs`, the
    /// constructors `ga`) as ours (`ma`): the same index, its leaves for
    /// the constructors it builds (theirs are the same shapes, `acceptable`
    /// leaves no boxed leaf a meet could have made), zeros for the others.
    fn resum(
        &self,
        call: Expr,
        gs: &Shape,
        ga: &[(String, Option<Vec<Shape>>)],
        ma: &[(String, Option<Vec<Shape>>)],
        next: &mut u32,
    ) -> Expr {
        let mut gl = Vec::new();
        gs.leaves(&mut gl);
        let gbs: Vec<Binder> = gl
            .iter()
            .map(|t| {
                let b = Binder::new(Id::new("r", *next), Mult::Many, t.clone());
                *next += 1;
                b
            })
            .collect();
        let mut out = vec![Expr::var(&gbs[0].id)];
        let mut gi = 1;
        for ((_, mf), (_, gf)) in ma.iter().zip(ga) {
            let gn = gf.as_deref().map_or(0, |fs| field_leaves(fs).len());
            if let Some(mf) = mf {
                let mts = field_leaves(mf);
                if gf.is_some() {
                    debug_assert_eq!(gn, mts.len(), "the same shape where both build");
                    out.extend(gbs[gi..gi + gn].iter().map(|b| Expr::var(&b.id)));
                } else {
                    out.extend(mts.iter().map(zero));
                }
            }
            gi += gn;
        }
        let rhs = self.ret(out);
        let gt = Type::Con(utup_name(gl.len()), gl);
        let cb = Binder::new(Id::new("u", *next), Mult::Many, gt);
        *next += 1;
        Expr::Case(
            Box::new(call),
            cb,
            self.wres.clone(),
            vec![Alt {
                con: AltCon::Con(utup_name(gbs.len()), gbs),
                rhs,
            }],
        )
    }

    fn ret(&self, leaves: Vec<Expr>) -> Expr {
        if leaves.len() == 1 {
            return leaves.into_iter().next().unwrap();
        }
        let ts = match self.wres {
            Type::Con(_, ts) => ts.clone(),
            _ => unreachable!(),
        };
        Expr::Con(utup_name(leaves.len()), ts, leaves)
    }

    fn expr(&self, e: Expr, env: &mut Env, next: &mut u32) -> Expr {
        match e {
            Expr::Let(Bind::NonRec(b, r), body) => {
                if b.mult == Mult::Many && matches!(&*r, Expr::Con(..)) {
                    env.insert(b.id.id, (*r).clone());
                }
                Expr::Let(Bind::NonRec(b, r), Box::new(self.expr(*body, env, next)))
            }
            Expr::LetStrict(b, r, body) => {
                if b.mult == Mult::Many && matches!(&*r, Expr::Con(..)) {
                    env.insert(b.id.id, (*r).clone());
                }
                Expr::LetStrict(b, r, Box::new(self.expr(*body, env, next)))
            }
            Expr::Let(bs, body) => Expr::Let(bs, Box::new(self.expr(*body, env, next))),
            Expr::Case(s, cb, _, alts) => Expr::Case(
                s,
                cb,
                self.wres.clone(),
                alts.into_iter()
                    .map(|a| Alt {
                        con: a.con,
                        rhs: self.expr(a.rhs, env, next),
                    })
                    .collect(),
            ),
            Expr::Panic(m, _) => Expr::Panic(m, self.wres.clone()),
            Expr::Cast(x, t) if same_result_call(&x, &t, self.me, self.cands) => {
                self.expr(*x, env, next)
            }
            e => {
                let Some(g) = cand_call(&e, self.me, self.cands).map(str::to_string) else {
                    let mut out = Vec::new();
                    leaves_of(self.shape, e, env, &mut out);
                    return self.ret(out);
                };
                let gs = &self.plans[&g];
                let call = retarget(e, &self.workers[&g]);
                if gs.same(self.shape) {
                    return call;
                }
                if let (Shape::Sum { alts: ga, .. }, Shape::Sum { alts: ma, .. }) = (gs, self.shape)
                {
                    return self.resum(call, gs, ga, ma, next);
                }
                // Another shape: take its results apart, rebuild the value
                // and return the leaves of ours.
                let mut gl = Vec::new();
                gs.leaves(&mut gl);
                let gbs: Vec<Binder> = gl
                    .iter()
                    .map(|t| {
                        let m = if linear(self.p, t, &self.cands[self.me].tvs, 0) {
                            Mult::One
                        } else {
                            Mult::Many
                        };
                        let b = Binder::new(Id::new("r", *next), m, t.clone());
                        *next += 1;
                        b
                    })
                    .collect();
                let mut it = gbs.iter().map(|b| Expr::var(&b.id));
                let value = build(gs, &mut it);
                let mut out = Vec::new();
                leaves_of(self.shape, value, env, &mut out);
                let rhs = self.ret(out);
                if gbs.len() == 1 {
                    Expr::LetStrict(gbs[0].clone(), Box::new(call), Box::new(rhs))
                } else {
                    let gt = Type::Con(utup_name(gl.len()), gl);
                    let cb = Binder::new(Id::new("u", *next), Mult::Many, gt);
                    *next += 1;
                    Expr::Case(
                        Box::new(call),
                        cb,
                        self.wres.clone(),
                        vec![Alt {
                            con: AltCon::Con(utup_name(gbs.len()), gbs),
                            rhs,
                        }],
                    )
                }
            }
        }
    }
}

/// `g @ts args` with `g` replaced by `w`.
fn retarget(e: Expr, w: &str) -> Expr {
    match e {
        Expr::App(f, a) => Expr::App(Box::new(retarget(*f, w)), a),
        Expr::TyApp(f, t) => Expr::TyApp(Box::new(retarget(*f, w)), t),
        Expr::Global(_) => Expr::Global(w.to_string()),
        e => e,
    }
}

fn fun_type(tvs: &[(TyVar, Kind)], xs: &[Binder], res: &Type) -> Type {
    let t = xs
        .iter()
        .rev()
        .fold(res.clone(), |r, x| Type::fun(x.mult, x.ty.clone(), r));
    tvs.iter().rev().fold(t, |t, (v, k)| {
        Type::Forall(v.clone(), k.clone(), Box::new(t))
    })
}

fn abstract_fun(tvs: &[(TyVar, Kind)], xs: &[Binder], body: Expr) -> Expr {
    let e = xs
        .iter()
        .rev()
        .fold(body, |b, x| Expr::Lam(x.clone(), Box::new(b)));
    tvs.iter().rev().fold(e, |b, (v, k)| {
        Expr::TyLam(v.clone(), k.clone(), Box::new(b))
    })
}
