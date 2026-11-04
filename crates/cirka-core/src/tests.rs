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
