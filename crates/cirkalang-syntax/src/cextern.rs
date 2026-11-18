//! Declarations of `extern` blocks other than functions (spec-v0.0.6.md
//! §1.1, §1.3): expanded on the AST, after their block, into data types,
//! definitions and `impl`s, like `deriving`. The code names `Foreign` and
//! the Prelude qualified; the resolver makes `Foreign` visible qualified in
//! a module with such declarations.
//!
//! - `type T`: `data T : Type` without constructors.
//! - `type T = t`: `T : Type; T = t`.
//! - `const X : t = lit`: `X : t; X = lit` (literals of the C types,
//!   spec-v0.0.6.md §1.2).
//! - `enum E : t where A = n`: `data E = MkE t`, `A : E`, `Storable E`.
//! - `struct S where f : t`: `data S = MkS t ..`, the field functions
//!   `S.f`, `S.set_f` and `S.ptr_f`, `Storable S`.
//! - `union U where m : t`: `data U = MkU (List CUChar)` (the bytes), the
//!   member pointers `U.m`, `Storable U`, `Eq U` (the bytes of its size).
//!
//! The layout of structs and unions is the compiler's (`primCLayout`).
//! `S.$peek` and `S.$poke` read and write a value at an address (`Int`):
//! the lowering calls them to pass values to C.

use crate::ast::*;
use cirka_diagnostics::Span;

/// The function reading a value of struct or union `t` at an address.
pub fn peek_name(t: &str) -> String {
    format!("{t}.$peek")
}

/// The function writing a value of struct or union `t` at an address.
pub fn poke_name(t: &str) -> String {
    format!("{t}.$poke")
}

/// The field function `S.f` of a struct, the member pointer `U.m` of a
/// union.
pub fn member_name(t: &str, f: &str) -> String {
    format!("{t}.{f}")
}

/// The constructor of struct, union or enum `t`.
pub fn con_name(t: &str) -> String {
    format!("Mk{t}")
}

/// Expands the declarations of the extern blocks of `m`. Returns whether
/// there were any (the module then uses `Foreign`).
pub fn expand(m: &mut Module) -> bool {
    let mut any = false;
    let mut out = Vec::with_capacity(m.decls.len());
    for d in std::mem::take(&mut m.decls) {
        let generated: Vec<Decl> = match &d.kind {
            DeclKind::Extern(b) if !b.items.is_empty() => {
                any = true;
                b.items.iter().flat_map(|i| item(i, d.private)).collect()
            }
            _ => Vec::new(),
        };
        out.push(d);
        out.extend(generated);
    }
    m.decls = out;
    any
}

fn item(i: &ExternItem, private: bool) -> Vec<Decl> {
    let sp = i.span;
    let decl = |kind| Decl {
        private,
        origin: Origin::Extern,
        kind,
        span: sp,
    };
    match &i.kind {
        ExternItemKind::Opaque => vec![decl(DeclKind::Data(DataDecl {
            name: i.name.clone(),
            name_span: i.name_span,
            params: Vec::new(),
            sig: Some(Expr {
                kind: ExprKind::Type,
                span: i.name_span,
            }),
            cons: Vec::new(),
            short: false,
            deriving: Vec::new(),
        }))],
        ExternItemKind::Alias(t) => vec![
            decl(sig(
                &i.name,
                i.name_span,
                Expr {
                    kind: ExprKind::Type,
                    span: i.name_span,
                },
            )),
            decl(clause(&i.name, i.name_span, Vec::new(), t.clone())),
        ],
        ExternItemKind::Const { ty, value } => vec![
            decl(sig(&i.name, i.name_span, ty.clone())),
            decl(clause(&i.name, i.name_span, Vec::new(), value.clone())),
        ],
        ExternItemKind::Enum { repr, consts } => enum_(i, repr, consts, private),
        ExternItemKind::Record {
            union: false,
            fields,
            ..
        } => struct_(i, fields, private),
        ExternItemKind::Record {
            union: true,
            fields,
            ..
        } => union_(i, fields, private),
    }
}

fn enum_(i: &ExternItem, repr: &Expr, consts: &[ExternConst], private: bool) -> Vec<Decl> {
    let sp = i.span;
    let con = con_name(&i.name);
    // Checked eagerly: the constants and `Storable` check the
    // representation.
    let decl = |kind| Decl {
        private,
        origin: Origin::Extern,
        kind,
        span: sp,
    };
    let mut out = vec![decl(DeclKind::Data(DataDecl {
        name: i.name.clone(),
        name_span: i.name_span,
        params: Vec::new(),
        sig: None,
        cons: vec![ConDecl {
            private: false,
            name: con.clone(),
            span: i.name_span,
            body: ConBody::Fields(vec![repr.clone()]),
        }],
        short: true,
        deriving: ["Eq", "Ord", "Show"]
            .iter()
            .map(|t| (prelude_name(t), sp))
            .collect(),
    }))];
    for c in consts {
        out.push(decl(sig(&c.name, c.span, var(&i.name, c.span))));
        out.push(decl(clause(
            &c.name,
            c.span,
            Vec::new(),
            app(var(&con, c.span), c.value.clone(), c.span),
        )));
    }
    // The value is stored as its integer.
    let as_repr = |p: Expr| {
        ann(
            app(foreign("castPtr", sp), p, sp),
            app(foreign("Ptr", sp), repr.clone(), sp),
            sp,
        )
    };
    let items = vec![
        method(
            "sizeOf",
            vec![pvar("p", sp)],
            app(foreign("sizeOf", sp), as_repr(var("p", sp)), sp),
            sp,
        ),
        method(
            "alignment",
            vec![pvar("p", sp)],
            app(foreign("alignment", sp), as_repr(var("p", sp)), sp),
            sp,
        ),
        method(
            "peek",
            vec![pvar("p", sp)],
            apps(
                prelude("fmap", sp),
                vec![
                    var(&con, sp),
                    app(foreign("peek", sp), as_repr(var("p", sp)), sp),
                ],
                sp,
            ),
            sp,
        ),
        method(
            "poke",
            vec![pvar("p", sp), pcon(&con, vec![pvar("v", sp)], sp)],
            apps(
                foreign("poke", sp),
                vec![as_repr(var("p", sp)), var("v", sp)],
                sp,
            ),
            sp,
        ),
        method(
            "zeroed",
            Vec::new(),
            app(var(&con, sp), foreign("zeroed", sp), sp),
            sp,
        ),
    ];
    out.push(storable(&i.name, items, Origin::Extern, sp));
    out
}

fn struct_(i: &ExternItem, fields: &[ExternField], private: bool) -> Vec<Decl> {
    let sp = i.span;
    let t = &i.name;
    let con = con_name(t);
    let n = fields.len();
    let decl = |kind| Decl {
        private,
        origin: Origin::ExternLazy,
        kind,
        span: sp,
    };
    let mut out = vec![decl(DeclKind::Data(DataDecl {
        name: t.clone(),
        name_span: i.name_span,
        params: Vec::new(),
        sig: None,
        cons: vec![ConDecl {
            private: false,
            name: con.clone(),
            span: i.name_span,
            body: ConBody::Fields(fields.iter().map(|f| f.ty.clone()).collect()),
        }],
        short: true,
        deriving: ["Eq", "Show"]
            .iter()
            .map(|c| (prelude_name(c), sp))
            .collect(),
    }))];
    let x = |k: usize| format!("x{k}");
    // `MkS x0 .. xn` with `_` except at `keep`.
    let con_pat = |keep: &dyn Fn(usize) -> bool, fsp: Span| {
        pcon(
            &con,
            (0..n)
                .map(|k| {
                    if keep(k) {
                        pvar(&x(k), fsp)
                    } else {
                        pwild(fsp)
                    }
                })
                .collect(),
            fsp,
        )
    };
    let self_ptr = |s: Span| {
        ann(
            foreign("nullPtr", s),
            app(foreign("Ptr", s), var(t, s), s),
            s,
        )
    };
    // The offset of field `k`: `primCLayout (nullPtr : Ptr S) (2 + k)`.
    let offset = |k: usize, s: Span| {
        apps(
            foreign("primCLayout", s),
            vec![self_ptr(s), int(2 + k as u128, s)],
            s,
        )
    };
    for (k, f) in fields.iter().enumerate() {
        let fsp = f.span;
        // S.f : S -> T
        let getter = member_name(t, &f.name);
        out.push(decl(sig(
            &getter,
            fsp,
            arrow(var(t, fsp), f.ty.clone(), fsp),
        )));
        out.push(decl(clause(
            &getter,
            fsp,
            vec![con_pat(&|j| j == k, fsp)],
            var(&x(k), fsp),
        )));
        // S.set_f : T -> S -> S
        let setter = member_name(t, &format!("set_{}", f.name));
        out.push(decl(sig(
            &setter,
            fsp,
            arrow(f.ty.clone(), arrow(var(t, fsp), var(t, fsp), fsp), fsp),
        )));
        out.push(decl(clause(
            &setter,
            fsp,
            vec![pvar("v", fsp), con_pat(&|j| j != k, fsp)],
            apps(
                var(&con, fsp),
                (0..n)
                    .map(|j| {
                        if j == k {
                            var("v", fsp)
                        } else {
                            var(&x(j), fsp)
                        }
                    })
                    .collect(),
                fsp,
            ),
        )));
        // S.ptr_f : Ptr S -> Ptr T
        let ptr = member_name(t, &format!("ptr_{}", f.name));
        out.push(decl(sig(
            &ptr,
            fsp,
            arrow(
                app(foreign("Ptr", fsp), var(t, fsp), fsp),
                app(foreign("Ptr", fsp), f.ty.clone(), fsp),
                fsp,
            ),
        )));
        out.push(decl(clause(
            &ptr,
            fsp,
            vec![pvar("p", fsp)],
            apps(
                foreign("plusPtr", fsp),
                vec![var("p", fsp), offset(k, fsp)],
                fsp,
            ),
        )));
    }
    // S.$peek : Int -> IO S
    let mut stmts: Vec<Stmt> = fields
        .iter()
        .enumerate()
        .map(|(k, f)| {
            Stmt::Bind(
                x(k),
                f.span,
                ann(
                    apps(
                        foreign("peekField", f.span),
                        vec![var("a", f.span), offset(k, f.span)],
                        f.span,
                    ),
                    app(prelude("IO", f.span), f.ty.clone(), f.span),
                    f.span,
                ),
            )
        })
        .collect();
    stmts.push(Stmt::Expr(app(
        prelude("pure", sp),
        apps(var(&con, sp), (0..n).map(|k| var(&x(k), sp)).collect(), sp),
        sp,
    )));
    out.extend(peek_poke(t, private, sp, stmts, {
        // S.$poke : Int -> S -> IO ()
        let mut stmts: Vec<Stmt> = fields
            .iter()
            .enumerate()
            .map(|(k, f)| {
                Stmt::Expr(apps(
                    foreign("pokeField", f.span),
                    vec![var("a", f.span), offset(k, f.span), var(&x(k), f.span)],
                    f.span,
                ))
            })
            .collect();
        stmts.push(Stmt::Expr(app(
            prelude("pure", sp),
            Expr {
                kind: ExprKind::Unit,
                span: sp,
            },
            sp,
        )));
        (vec![con_pat(&|_| true, sp)], do_(stmts, sp))
    }));
    out.push(storable(
        t,
        layout_methods(
            sp,
            apps(
                var(&con, sp),
                (0..n).map(|_| foreign("zeroed", sp)).collect(),
                sp,
            ),
        ),
        Origin::ExternLazy,
        sp,
    ));
    out
}

fn union_(i: &ExternItem, fields: &[ExternField], private: bool) -> Vec<Decl> {
    let sp = i.span;
    let t = &i.name;
    let con = con_name(t);
    let decl = |kind| Decl {
        private,
        origin: Origin::ExternLazy,
        kind,
        span: sp,
    };
    let bytes = app(prelude("List", sp), foreign("CUChar", sp), sp);
    let mut out = vec![decl(DeclKind::Data(DataDecl {
        name: t.clone(),
        name_span: i.name_span,
        params: Vec::new(),
        sig: None,
        cons: vec![ConDecl {
            private: false,
            name: con.clone(),
            span: i.name_span,
            body: ConBody::Fields(vec![bytes]),
        }],
        short: true,
        deriving: vec![(prelude_name("Show"), sp)],
    }))];
    // U.m : Ptr U -> Ptr T
    for f in fields {
        let name = member_name(t, &f.name);
        out.push(decl(sig(
            &name,
            f.span,
            arrow(
                app(foreign("Ptr", f.span), var(t, f.span), f.span),
                app(foreign("Ptr", f.span), f.ty.clone(), f.span),
                f.span,
            ),
        )));
        out.push(decl(clause(
            &name,
            f.span,
            vec![pvar("p", f.span)],
            app(foreign("castPtr", f.span), var("p", f.span), f.span),
        )));
    }
    let size = apps(
        foreign("primCLayout", sp),
        vec![
            ann(
                foreign("nullPtr", sp),
                app(foreign("Ptr", sp), var(t, sp), sp),
                sp,
            ),
            int(0, sp),
        ],
        sp,
    );
    let at = app(foreign("MkPtr", sp), var("a", sp), sp);
    let peek = app(
        app(prelude("fmap", sp), var(&con, sp), sp),
        apps(foreign("peekArray", sp), vec![size.clone(), at.clone()], sp),
        sp,
    );
    let poke = (
        vec![pcon(&con, vec![pvar("bs", sp)], sp)],
        apps(
            foreign("pokeArrayN", sp),
            vec![at, size.clone(), var("bs", sp)],
            sp,
        ),
    );
    out.extend(peek_poke(t, private, sp, vec![Stmt::Expr(peek)], poke));
    let zeroed = app(
        var(&con, sp),
        app(foreign("zeroedList", sp), size.clone(), sp),
        sp,
    );
    out.push(storable(
        t,
        layout_methods(sp, zeroed),
        Origin::ExternLazy,
        sp,
    ));
    // Equal when their bytes are: `resize size a == resize size b`.
    let bytes_of = |x: &str| apps(foreign("resize", sp), vec![size.clone(), var(x, sp)], sp);
    out.push(Decl {
        private: false,
        origin: Origin::ExternLazy,
        kind: DeclKind::Impl(ImplDecl {
            binders: Vec::new(),
            context: Vec::new(),
            head: app(prelude("Eq", sp), var(t, sp), sp),
            items: vec![method(
                "==",
                vec![
                    pcon(&con, vec![pvar("a", sp)], sp),
                    pcon(&con, vec![pvar("b", sp)], sp),
                ],
                apps(prelude("==", sp), vec![bytes_of("a"), bytes_of("b")], sp),
                sp,
            )],
        }),
        span: sp,
    });
    out
}

/// `T.$peek a = do stmts` and `T.$poke a pats = poke`.
fn peek_poke(
    t: &str,
    private: bool,
    sp: Span,
    peek: Vec<Stmt>,
    poke: (Vec<Pat>, Expr),
) -> Vec<Decl> {
    let decl = |kind| Decl {
        private,
        origin: Origin::ExternLazy,
        kind,
        span: sp,
    };
    let int_ty = prelude("Int", sp);
    let io = |r: Expr| app(prelude("IO", sp), r, sp);
    let (poke_pats, poke_body) = poke;
    let mut pats = vec![pvar("a", sp)];
    pats.extend(poke_pats);
    vec![
        decl(sig(
            &peek_name(t),
            sp,
            arrow(int_ty.clone(), io(var(t, sp)), sp),
        )),
        decl(clause(
            &peek_name(t),
            sp,
            vec![pvar("a", sp)],
            do_(peek, sp),
        )),
        decl(sig(
            &poke_name(t),
            sp,
            arrow(
                int_ty,
                arrow(
                    var(t, sp),
                    io(Expr {
                        kind: ExprKind::Unit,
                        span: sp,
                    }),
                    sp,
                ),
                sp,
            ),
        )),
        decl(clause(&poke_name(t), sp, pats, poke_body)),
    ]
}

/// The methods of `Storable` of a struct or union: the layout is the
/// compiler's, reading and writing are `T.$peek` and `T.$poke`.
fn layout_methods(sp: Span, zeroed: Expr) -> Vec<Decl> {
    vec![
        method(
            "sizeOf",
            vec![pvar("p", sp)],
            apps(
                foreign("primCLayout", sp),
                vec![var("p", sp), int(0, sp)],
                sp,
            ),
            sp,
        ),
        method(
            "alignment",
            vec![pvar("p", sp)],
            apps(
                foreign("primCLayout", sp),
                vec![var("p", sp), int(1, sp)],
                sp,
            ),
            sp,
        ),
        method("zeroed", Vec::new(), zeroed, sp),
    ]
}

/// `impl Foreign.Storable T where items` with `peek` and `poke` through
/// `T.$peek` and `T.$poke` added for structs and unions.
fn storable(t: &str, mut items: Vec<Decl>, origin: Origin, sp: Span) -> Decl {
    if !items
        .iter()
        .any(|d| matches!(&d.kind, DeclKind::Clause { name, .. } if name == "peek"))
    {
        let addr = app(foreign("ptrAddress", sp), var("p", sp), sp);
        items.push(method(
            "peek",
            vec![pvar("p", sp)],
            app(var(&peek_name(t), sp), addr.clone(), sp),
            sp,
        ));
        items.push(method(
            "poke",
            vec![pvar("p", sp), pvar("v", sp)],
            apps(var(&poke_name(t), sp), vec![addr, var("v", sp)], sp),
            sp,
        ));
    }
    Decl {
        private: false,
        origin,
        kind: DeclKind::Impl(ImplDecl {
            binders: Vec::new(),
            context: Vec::new(),
            head: app(foreign("Storable", sp), var(t, sp), sp),
            items,
        }),
        span: sp,
    }
}

// ---- AST construction ----

fn prelude_name(n: &str) -> QName {
    QName {
        module: vec!["Prelude".into()],
        name: n.into(),
    }
}

pub(crate) fn prelude(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(prelude_name(n)),
        span,
    }
}

fn foreign(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName {
            module: vec!["Foreign".into()],
            name: n.into(),
        }),
        span,
    }
}

pub(crate) fn var(n: &str, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Var(QName::simple(n)),
        span,
    }
}

fn int(n: u128, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Lit(Lit::Int(n)),
        span,
    }
}

pub(crate) fn app(f: Expr, a: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::App(Box::new(f), Box::new(a)),
        span,
    }
}

pub(crate) fn apps(f: Expr, args: Vec<Expr>, span: Span) -> Expr {
    args.into_iter().fold(f, |f, a| app(f, a, span))
}

fn ann(e: Expr, t: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Ann(Box::new(e), Box::new(t)),
        span,
    }
}

pub(crate) fn arrow(a: Expr, b: Expr, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Pi {
            mult: None,
            implicit: false,
            names: Vec::new(),
            dom: Box::new(a),
            cod: Box::new(b),
        },
        span,
    }
}

fn do_(stmts: Vec<Stmt>, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Do(stmts),
        span,
    }
}

pub(crate) fn pvar(n: &str, span: Span) -> Pat {
    Pat {
        kind: PatKind::Var(n.into()),
        span,
    }
}

fn pwild(span: Span) -> Pat {
    Pat {
        kind: PatKind::Wild,
        span,
    }
}

pub(crate) fn pcon(c: &str, args: Vec<Pat>, span: Span) -> Pat {
    Pat {
        kind: PatKind::Con(QName::simple(c), args),
        span,
    }
}

pub(crate) fn sig(name: &str, span: Span, ty: Expr) -> DeclKind {
    DeclKind::Sig {
        names: vec![(name.to_string(), span)],
        ty,
        totality: None,
        trusted: None,
    }
}

pub(crate) fn clause(name: &str, span: Span, pats: Vec<Pat>, rhs: Expr) -> DeclKind {
    DeclKind::Clause {
        name: name.to_string(),
        name_span: span,
        pats,
        rhs,
        measure: None,
    }
}

fn method(name: &str, pats: Vec<Pat>, rhs: Expr, span: Span) -> Decl {
    Decl {
        private: false,
        origin: Origin::Source,
        kind: clause(name, span, pats, rhs),
        span,
    }
}
