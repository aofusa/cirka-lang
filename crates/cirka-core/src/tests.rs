use crate::build::B;
use crate::*;
use cirka_diagnostics::FileId;

#[test]
fn mult_semiring() {
    use Mult::*;
    assert_eq!(One.plus(One), Many);
    assert_eq!(Zero.plus(One), One);
    assert_eq!(Zero.times(Many), Zero);
    assert_eq!(One.times(Many), Many);
    assert_eq!(One.times(One), One);
}

#[test]
fn substitution_avoids_capture() {
    // (forall (b) (-> a b))[b/a] must not capture the inner b.
    let a = TyVar::new("a", 1);
    let b = TyVar::new("b", 2);
    let t = Type::Forall(
        b.clone(),
        Kind::Type,
        Box::new(Type::fun(
            Mult::Many,
            Type::Var(a.clone()),
            Type::Var(b.clone()),
        )),
    );
    let r = t.subst(&a, &Type::Var(b.clone()));
    let Type::Forall(b2, _, body) = &r else {
        panic!()
    };
    assert_ne!(b2, &b);
    assert!(body.alpha_eq(&Type::fun(
        Mult::Many,
        Type::Var(b.clone()),
        Type::Var(b2.clone())
    )));
}

#[test]
fn alpha_equivalence() {
    let t1 = parse::parse_type(FileId(0), "(forall (a) (-> a a))").unwrap();
    let t2 = parse::parse_type(FileId(0), "(forall (b) (-> b b))").unwrap();
    let t3 = parse::parse_type(FileId(0), "(forall (b) (-> 1 b b))").unwrap();
    assert!(t1.alpha_eq(&t2));
    assert!(!t1.alpha_eq(&t3));
}

#[test]
fn higher_kinded_application_normalizes() {
    let f = TyVar::new("f", 1);
    let t = Type::mk_app(Type::Var(f.clone()), vec![Type::int()]);
    let r = t.subst(&f, &Type::Con("Maybe".into(), vec![]));
    assert_eq!(r, Type::Con("Maybe".into(), vec![Type::int()]));
}

/// The building DSL produces programs that Lint accepts, and Lint rejects
/// a U-Bind violation built the same way.
#[test]
fn dsl_builds_linted_programs() {
    let mut b = B::new();
    let handle = DataDecl {
        name: "Handle".into(),
        params: vec![],
        linear: true,
        ctors: vec![CtorDecl {
            name: "MkHandle".into(),
            fields: vec![Type::int()],
            strict: vec![false],
        }],
        newtype: false,
    };
    let hty = Type::con("Handle");
    let mk = |b: &mut B, m: Mult| {
        let h = b.binder("h", m, hty.clone());
        let s = b.binder("s", Mult::One, hty.clone());
        let fd = b.binder("fd", Mult::Many, Type::int());
        B::lam(
            &h,
            B::case(
                Expr::var(&h.id),
                &s,
                Type::int(),
                vec![B::alt_con("MkHandle", vec![fd.clone()], Expr::var(&fd.id))],
            ),
        )
    };
    let good = mk(&mut b, Mult::One);
    let bad = mk(&mut b, Mult::Many);
    let prog = |e: Expr, m: Mult| Program {
        datas: vec![handle.clone()],
        externs: vec![],
        collect_cycles: false,
        binds: vec![
            TopBind {
                name: "close".into(),
                ty: Type::fun(m, hty.clone(), Type::int()),
                expr: e,
                span: cirka_diagnostics::Span::DUMMY,
            },
            TopBind {
                name: "main".into(),
                ty: Type::int(),
                expr: Expr::int(0),
                span: cirka_diagnostics::Span::DUMMY,
            },
        ],
    };
    assert!(lint::lint_program(&prog(good, Mult::One)).is_empty());
    let errs = lint::lint_program(&prog(bad, Mult::Many));
    assert!(
        lint::render_errors(&errs).contains("U-Bind"),
        "{}",
        lint::render_errors(&errs)
    );
}

#[test]
fn printer_disambiguates_shadowed_names() {
    let mut b = B::new();
    let x1 = b.binder("x", Mult::Many, Type::int());
    let x2 = b.binder("x", Mult::Many, Type::int());
    // \x -> \x -> x1 + x2  (both named x)
    let e = B::lam(
        &x1,
        B::lam(
            &x2,
            B::prim(PrimOp::Add, vec![Expr::var(&x1.id), Expr::var(&x2.id)]),
        ),
    );
    let s = print::print_expr(&e);
    assert!(s.contains("x~1") && s.contains("x~2"), "{s}");
}

/// Optimizes `src` and returns the printed `main`.
fn opt_main(src: &str) -> (String, opt::Stats) {
    let mut p = parse_program(FileId(0), src).unwrap();
    let errs = lint::lint_program(&p);
    assert!(errs.is_empty(), "{errs:?}");
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let m = p.bind("main").unwrap();
    (print::print_expr(&m.expr), st)
}

const PRELUDE: &str = "(data Int () (I# Int#))\n(data Pair (a b) (P a b))\n";

#[test]
fn opt_beta_and_constant_folding() {
    let src = format!(
        "{PRELUDE}(def main Int# ((lam ((x Int#) (y Int#)) (prim add x (prim mul y 2))) 1 20))"
    );
    let (m, st) = opt_main(&src);
    assert_eq!(m.trim(), "41");
    assert!(st.beta >= 2 && st.folded >= 2, "{st:?}");
}

#[test]
fn opt_inlines_small_functions_and_removes_them() {
    let src = format!(
        "{PRELUDE}(def unI (-> Int Int#) (lam ((x Int)) (case x (s Int) Int# ((I# v) v))))
         (def main Int# (unI (con I# () 7)))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert_eq!(print::print_expr(&p.bind("main").unwrap().expr).trim(), "7");
    assert_eq!(st.removed_globals, 1);
    assert!(p.bind("unI").is_none());
}

#[test]
fn opt_case_of_known_constructor() {
    // fst (P a b) with a lazy, non-atomic second component: the pair and
    // the thunk for the unused field disappear.
    let src = format!(
        "{PRELUDE}(def main Int#
           (case (con P (Int Int) (con I# () 3) (con I# () (prim add 1 2))) (p (Pair Int Int)) Int#
             ((P a b) (case a (s Int) Int# ((I# v) v)))))"
    );
    let (m, st) = opt_main(&src);
    assert_eq!(m.trim(), "3");
    assert!(st.known_case >= 2, "{st:?}");
}

#[test]
fn opt_keeps_strictness_and_effects() {
    // `let!` of an extern call and of a possibly-diverging variable must
    // stay; an unused lazy `let` goes away.
    let src = format!(
        "{PRELUDE}(extern f (Int#) Unit#)
         (def g Int (con I# () 1))
         (def main Int#
           (let ((dead Int (con I# () (prim add 1 1))))
             (let! (u Unit# (extern f 1))
               (let! (w Int g)
                 0))))"
    );
    let (m, st) = opt_main(&src);
    assert!(m.contains("extern f"), "{m}");
    assert!(m.contains("(w Int g)"), "{m}");
    assert!(!m.contains("dead"), "{m}");
    assert!(st.dead >= 1);
}

#[test]
fn opt_single_use_thunk_moves_to_its_use() {
    // A lazy let used once (not under a lambda) is moved to its use; one
    // used under a lambda is kept (work would be repeated).
    let src = format!(
        "{PRELUDE}(def h (-> Int Int) (lam ((x Int)) x))
         (def main Int#
           (let ((t Int (h (con I# () 5))))
             (let ((k (-> Int# Int) (lam ((z Int#)) t)))
               (case (k 0) (s Int) Int# ((I# v) v)))))"
    );
    let (m, _) = opt_main(&src);
    assert_eq!(m.trim(), "5", "{m}");
}

#[test]
fn opt_respects_linearity() {
    // A linear argument that is not a value is substituted at its single
    // use instead of becoming a lazy let (Strict-Linear).
    let src = format!(
        "{PRELUDE}(def id1 (-> 1 Int Int) (lam ((x 1 Int)) x))
         (def big (-> Int Int) (lam ((y Int)) y))
         (def main Int#
           ((lam ((q 1 Int)) (case (id1 q) (s Int) Int# ((I# v) v))) (big (con I# () 9))))"
    );
    let (m, _) = opt_main(&src);
    assert_eq!(m.trim(), "9", "{m}");
}

const STRICT_INT: &str = "(data Int () (I# (! Int#)))\n";

#[test]
fn opt_worker_wrapper_unboxes_strict_loops() {
    // A loop strict in two boxed accumulators: the worker takes and
    // returns `Int#`, and no `I#` is built inside it.
    let src = format!(
        "{STRICT_INT}(def sum (-> Int Int Int)
           (lam ((n Int) (acc Int))
             (case n (s Int) Int
               ((I# k) (case k (kk Int#) Int
                 (0 acc)
                 (_ (case acc (t Int) Int
                   ((I# a) (sum (con I# () (prim sub k 1)) (con I# () (prim add a k)))))))))))
         (extern arg () Int#)
         (def main Int#
           (let! (x Int# (extern arg))
             (case (sum (con I# () x) (con I# () 0)) (r Int) Int# ((I# v) v))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert_eq!(st.worker_wrapper, 1, "{st:?}");
    let w = p.bind("sum$w").expect("worker");
    let s = print::print_expr(&w.expr);
    assert!(!s.contains("I#"), "{s}");
    assert_eq!(w.ty.to_string(), "(-> Int# (-> Int# Int#))");
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(m.contains("sum$w") && !m.contains("I#"), "{m}");
}

#[test]
fn opt_worker_wrapper_unboxes_parameters_passed_evaluated() {
    // `loop` returns `acc` without using `step` when `n` is 0, so it is not
    // strict in `step`; but every call passes `step` evaluated (a
    // constructor, or `step` itself), so `step` is unboxed as well
    // (FEAT-0056).
    let src = format!(
        "{STRICT_INT}(def loop (-> Int# Int Int Int)
           (lam ((n Int#) (step Int) (acc Int))
             (case n (k Int#) Int
               (0 acc)
               (_ (case step (s Int) Int
                 ((I# d) (case acc (t Int) Int
                   ((I# a) (loop (prim sub n 1) step (con I# () (prim add a d)))))))))))
         (extern arg () Int#)
         (def main Int#
           (let! (x Int# (extern arg))
             (case (loop x (con I# () x) (con I# () 0)) (r Int) Int# ((I# v) v))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert_eq!(st.worker_wrapper, 1, "{st:?}");
    let w = p.bind("loop$w").expect("worker");
    assert_eq!(w.ty.to_string(), "(-> Int# (-> Int# (-> Int# Int#)))");
    assert!(!print::print_expr(&w.expr).contains("I#"));
}

#[test]
fn worker_wrapper_keeps_lazy_parameters_passed_unevaluated() {
    // A caller passes a lazily bound, possibly failing value for the
    // non-strict `step`: evaluating it before the call could fail, so
    // `step` stays boxed.
    let src = format!(
        "{STRICT_INT}(def loop (-> Int# Int Int Int)
           (lam ((n Int#) (step Int) (acc Int))
             (case n (k Int#) Int
               (0 acc)
               (_ (case step (s Int) Int
                 ((I# d) (case acc (t Int) Int
                   ((I# a) (loop (prim sub n 1) step (con I# () (prim add a d)))))))))))
         (extern arg () Int#)
         (def main Int#
           (let! (x Int# (extern arg))
             (let ((bad Int (case x (k Int#) Int (0 (panic \"zero\" Int)) (_ (con I# () x)))))
               (case (loop x bad (con I# () 0)) (r Int) Int# ((I# v) v)))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let ev = crate::ww::evaluated_params(&p);
    assert_eq!(ev["loop"], vec![true, false, true]);
    opt::optimize(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let w = p.bind("loop$w").expect("worker");
    assert_eq!(w.ty.to_string(), "(-> Int# (-> Int (-> Int# Int#)))");
}

#[test]
fn opt_unpacks_strict_boxed_fields() {
    let src = format!(
        "{STRICT_INT}(data V () (V (! Int) (! Int)) (W Int))
         (extern arg () Int#)
         (def main Int#
           (let! (x Int# (extern arg))
             (case (con V () (con I# () x) (con I# () (prim add x 1))) (v V) Int#
               ((V a b) (case a (s Int) Int# ((I# y) y)))
               ((W c) 0))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert_eq!(st.unpacked_fields, 2, "{st:?}");
    let v = p.data("V").unwrap();
    assert_eq!(v.ctors[0].fields[0].to_string(), "Int#");
    // The lazy field of `W` stays boxed.
    assert_eq!(v.ctors[1].fields[0].to_string(), "Int");
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(!m.contains("I#"), "{m}");
}

#[test]
fn opt_case_of_case() {
    // case (case c of 0 -> I# 1; _ -> I# 2) of I# v -> v + 10
    //   ⇒ case c of 0 -> 11; _ -> 12
    let src = format!(
        "{STRICT_INT}(extern arg () Int#)
         (def main Int#
           (let! (c Int# (extern arg))
             (case (case c (k Int#) Int (0 (con I# () 1)) (_ (con I# () 2))) (s Int) Int#
               ((I# v) (prim add v 10)))))"
    );
    let (m, st) = opt_main(&src);
    assert!(!m.contains("I#"), "{m}");
    assert!(m.contains("11") && m.contains("12"), "{m}");
    assert!(st.known_case >= 3, "{st:?}");
}

// ---- FEAT-0112: lambda lifting, specialization, strictify, float-in ----

#[test]
fn opt_lambda_lifts_saturated_local_loops() {
    // A local loop capturing `n` becomes a global taking `n`; worker/wrapper
    // then unboxes its strict accumulator.
    let src = format!(
        "{STRICT_INT}(extern arg () Int#)
         (def main Int#
           (let! (x Int# (extern arg))
             (let ((n Int (con I# () x)))
               (letrec ((go (-> Int Int Int)
                          (lam ((acc Int) (i Int))
                            (case i (s Int) Int
                              ((I# k) (case n (t Int) Int
                                ((I# m) (case (prim gt k m) (c Int#) Int
                                  (0 (case acc (u Int) Int
                                    ((I# a) (go (con I# () (prim add a k)) (con I# () (prim add k 1))))))
                                  (_ acc)))))))))
                 (case (go (con I# () 0) (con I# () 1)) (r Int) Int# ((I# v) v))))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert_eq!(st.lifted, 1, "{st:?}");
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(!m.contains("letrec") && m.contains("main$go"), "{m}");
    assert!(
        p.binds
            .iter()
            .any(|b| b.name.starts_with("main$go") && b.name.ends_with("$w"))
    );
}

#[test]
fn lambda_lift_keeps_functions_used_as_values() {
    // `go` is passed to `twice` unapplied: lifting would build a partial
    // application at every use, so the closure stays.
    let src = format!(
        "{PRELUDE}(def twice (-> (-> Int# Int#) Int# Int#) (lam ((f (-> Int# Int#)) (x Int#)) (f (f x))))
         (def main Int#
           (let! (k Int# 7)
             (letrec ((go (-> Int# Int#) (lam ((i Int#)) (prim add i k))))
               (twice go 1))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let mut next = p.max_id() + 1;
    assert_eq!(strict::lambda_lift(&mut p, &mut next), 0);
}

#[test]
fn opt_specializes_constant_dictionaries() {
    // `double` is overloaded over a one-method class; called with the
    // constant dictionary it is specialized and the method call resolved.
    let src = format!(
        "{STRICT_INT}(data Add (a) (Add$Dict (-> a a a)))
         (def plusInt (-> Int Int Int)
           (lam ((x Int) (y Int))
             (case x (s Int) Int ((I# a) (case y (t Int) Int ((I# b) (con I# () (prim add a b))))))))
         (def dAddInt (Add Int) (con Add$Dict (Int) plusInt))
         (def double (forall (a) (-> (Add a) Int# a a))
           (tlam (a) (lam ((d (Add a)) (n Int#) (x a))
             (case n (k Int#) a
               (0 x)
               (_ (case d (s (Add a)) a
                 ((Add$Dict op) ((@ double a) d (prim sub n 1) (op x x)))))))))
         (extern arg () Int#)
         (def main Int#
           (let! (c Int# (extern arg))
             (case ((@ double Int) dAddInt c (con I# () 1)) (r Int) Int# ((I# v) v))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert!(st.specialized >= 1, "{st:?}");
    let all = print_program(&p);
    // The specialized loop is monomorphic and unboxed.
    let spec = p
        .binds
        .iter()
        .find(|b| b.name.starts_with("double$s") && b.name.ends_with("$w"))
        .unwrap_or_else(|| panic!("{all}"));
    let s = print::print_expr(&spec.expr);
    assert!(!s.contains("dAddInt") && !s.contains("I#"), "{s}");
}

#[test]
fn opt_specializes_lifted_overloaded_local_loops() {
    // An overloaded local loop capturing `k`, called with a constant
    // dictionary from a second local function: lifting the outer group
    // first keeps the calls saturated, the lifted function takes its
    // dictionary first, and specialization makes it monomorphic.
    let src = format!(
        "{STRICT_INT}(data Add (a) (Add$Dict (-> a a a)))
         (def plusInt (-> Int Int Int)
           (lam ((x Int) (y Int))
             (case x (s Int) Int ((I# a) (case y (t Int) Int ((I# b) (con I# () (prim add a b))))))))
         (def dAddInt (Add Int) (con Add$Dict (Int) plusInt))
         (extern arg () Int#)
         (def main Int#
           (let! (k Int# (extern arg))
             (letrec ((go (forall (a) (-> (Add a) Int# a a))
                        (tlam (a) (lam ((d (Add a)) (n Int#) (x a))
                          (case (prim lt n k) (c Int#) a
                            (0 x)
                            (_ (case d (s (Add a)) a
                              ((Add$Dict op) ((@ go a) d (prim add n 1) (op x x))))))))))
               (letrec ((run (-> Int# Int)
                          (lam ((m Int#)) ((@ go Int) dAddInt m (con I# () 1)))))
                 (case (run 0) (r Int) Int# ((I# v) v))))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let st = opt::optimize_with_stats(&mut p).unwrap();
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert!(st.lifted >= 1 && st.specialized >= 1, "{st:?}");
    let all = print_program(&p);
    let spec = p
        .binds
        .iter()
        .find(|b| b.name.starts_with("main$go$s") && b.name.ends_with("$w"))
        .unwrap_or_else(|| panic!("{all}"));
    let s = print::print_expr(&spec.expr);
    assert!(!s.contains("dAddInt") && !s.contains("I#"), "{s}");
}

#[test]
fn strictify_evaluates_strict_arguments_eagerly() {
    // `len` is strict in its list; the non-atomic argument `(build 3)` is
    // evaluated with `let!` instead of being passed as a thunk.
    let src = format!(
        "{PRELUDE}(data L () (N) (C Int# L))
         (def build (-> Int# L)
           (lam ((n Int#)) (case n (k Int#) L (0 (con N ())) (_ (con C () n (build (prim sub n 1)))))))
         (def len (-> L Int#)
           (lam ((l L)) (case l (s L) Int# ((N) 0) ((C x r) (prim add 1 (len r))))))
         (def main Int# (len (build 3)))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let mut next = p.max_id() + 1;
    let n = strict::strictify(&mut p, &mut next);
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    assert!(n >= 1);
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(m.starts_with("(let! (s"), "{m}");
}

#[test]
fn strictify_turns_forced_lets_strict() {
    let src = format!(
        "{PRELUDE}(def f (-> Int Int) (lam ((x Int)) x))
         (def main Int#
           (let ((t Int (f (con I# () 5))))
             (case t (s Int) Int# ((I# v) v))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    let mut next = p.max_id() + 1;
    assert_eq!(strict::strictify(&mut p, &mut next), 1);
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(m.starts_with("(let! (t"), "{m}");
}

#[test]
fn float_in_moves_lets_into_their_only_alternative() {
    let src = format!(
        "{PRELUDE}(extern arg () Int#)
         (def f Int
           (let! (x Int# (extern arg))
             (let ((b Int (con I# () x)))
               (let! (c Int# (prim eq x 0))
                 (case c (k Int#) Int (0 b) (_ (con I# () 1)))))))
         (def main Int# 0)"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    assert_eq!(strict::float_in(&mut p), 1);
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let m = print::print_expr(&p.bind("f").unwrap().expr);
    assert!(m.contains("(0 (let ((b"), "{m}");
}

#[test]
fn dictionary_fields_become_globals() {
    let src = format!(
        "{PRELUDE}(data D (a) (D$Dict (-> a a)))
         (def dInt (D Int) (con D$Dict (Int) (lam ((x Int)) x)))
         (def main Int# 0)"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    assert_eq!(strict::float_dict_fields(&mut p), 1);
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let d = print::print_expr(&p.bind("dInt").unwrap().expr);
    assert!(d.contains("dInt$f0"), "{d}");
}

#[test]
fn opt_resolves_polymorphic_dictionary_under_type_application() {
    // An instance for a type constructor is `Λr. D$Dict …` with a lambda
    // field: the field moves to `dF$f0 = Λr. λx. x`, and a method call
    // through `dF @Int` is resolved and inlined (BUG-0011).
    let src = format!(
        "{PRELUDE}(data D (a) (D$Dict (-> a a)))
         (def dF (forall (r) (D r)) (tlam (r) (con D$Dict (r) (lam ((x r)) x))))
         (def main Int#
           (case (@ dF Int) (d (D Int)) Int#
             ((D$Dict f) (case (f (con I# () 5)) (s Int) Int# ((I# v) v)))))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    assert!(lint::lint_program(&p).is_empty());
    assert_eq!(strict::float_dict_fields(&mut p), 1);
    assert!(lint::lint_program(&p).is_empty(), "{}", print_program(&p));
    let f = print::print_expr(&p.bind("dF").unwrap().expr);
    assert!(f.contains("dF$f0"), "{f}");
    let (m, st) = opt_main(&src);
    assert_eq!(m.trim(), "5");
    assert!(st.known_case >= 1, "{st:?}");
}

#[test]
fn opt_applies_through_casts() {
    // `ret = cast retImpl` (IO's `primReturnIO` over `prim$io_return`) is
    // inlined as an alias, and the application goes through the cast.
    let src = format!(
        "{PRELUDE}(def retImpl (-> Int Int) (lam ((x Int)) x))
         (def ret (-> Int Int) (cast retImpl (-> Int Int)))
         (def main Int# (case (ret (con I# () 7)) (s Int) Int# ((I# v) v)))"
    );
    let (m, _) = opt_main(&src);
    assert_eq!(m.trim(), "7");
}

#[test]
fn opt_gives_io_functions_the_state_token() {
    // `loop` returns an IO action (a newtype over `Unit# -> a`): it gets a
    // worker `loop$io` taking the token, and running `loop 3` becomes a
    // direct call without building the action's closure (BUG-0011).
    let src = format!(
        "{PRELUDE}(data IO (a) :newtype (IO# (-> Unit# a)))
         (def ret (-> Int (IO Int)) (lam ((x Int)) (cast (lam ((w Unit#)) x) (IO Int))))
         (def loop (-> Int# (IO Int))
           (lam ((n Int#))
             (case n (k Int#) (IO Int)
               (0 (ret (con I# () 7)))
               (_ (loop (prim sub n 1))))))
         (def main Int#
           (case ((cast (loop 3) (-> Unit# Int)) unit) (r Int) Int# ((I# v) v)))"
    );
    let mut p = parse_program(FileId(0), &src).unwrap();
    assert!(lint::lint_program(&p).is_empty());
    let st = opt::optimize_with_stats(&mut p).unwrap();
    let all = print_program(&p);
    assert!(lint::lint_program(&p).is_empty(), "{all}");
    assert!(st.io_arity >= 1, "{st:?}");
    let m = print::print_expr(&p.bind("main").unwrap().expr);
    assert!(m.contains("(loop$io 3 unit)"), "{m}");
}

// ---- FEAT-0123: evaluated scrutinees, value fields, let! of a case ----

#[test]
fn opt_default_case_of_evaluated_variable() {
    // let! x = mk 1 in case x of _ -> ...  ⇒  the default-only `case` (the
    // `seq` of a bang pattern) disappears.
    let src = format!(
        "{PRELUDE}(extern mk (Int#) Int)
         (def main Int#
           (let! (x Int (extern mk 1))
             (case x (s Int) Int# (_ (case s (t Int) Int# ((I# v) v))))))"
    );
    let (m, st) = opt_main(&src);
    assert_eq!(m.matches("(case").count(), 1, "{m}");
    assert!(st.known_case >= 1, "{st:?}");
}

#[test]
fn opt_let_strict_of_constructor_with_value_fields() {
    // let! p = P (I# x) y in case p of P a b -> a  ⇒  the pair is never
    // built: the value field is floated out and `p` is a known constructor.
    let src = format!(
        "{PRELUDE}(extern arg () Int#)
         (extern mk (Int#) Int)
         (def main Int#
           (let! (x Int# (extern arg))
             (let! (y Int (extern mk x))
               (let! (p (Pair Int Int) (con P (Int Int) (con I# () x) y))
                 (case p (s (Pair Int Int)) Int# ((P a b) (case a (t Int) Int# ((I# v) v))))))))"
    );
    let (m, st) = opt_main(&src);
    assert!(!m.contains("con P"), "{m}");
    assert!(st.known_case >= 1, "{st:?}");
}

#[test]
fn opt_let_strict_of_single_alternative_case() {
    // let! t = (case q of P a b -> P b a) in case t of P c d -> c
    //   ⇒ case q of P a b -> b  (no intermediate pair)
    let src = format!(
        "{PRELUDE}(extern mkp (Int#) (Pair Int Int))
         (def main Int#
           (let! (q (Pair Int Int) (extern mkp 1))
             (let! (t (Pair Int Int) (case q (s (Pair Int Int)) (Pair Int Int) ((P a b) (con P (Int Int) b a))))
               (case t (s2 (Pair Int Int)) Int# ((P c d) (case c (u Int) Int# ((I# v) v)))))))"
    );
    let (m, st) = opt_main(&src);
    assert!(!m.contains("con P"), "{m}");
    assert!(st.floated >= 1 && st.known_case >= 1, "{st:?}");
}
