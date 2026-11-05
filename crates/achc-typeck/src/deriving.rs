//! Derived instances (Haskell 2010 §11): `Eq`, `Ord`, `Show`, `Read`,
//! `Enum` and `Bounded`, generated as instance declarations in renamed
//! form (global names qualified, locals unique) and then checked like
//! written ones. The code follows the report's specification; the helper
//! functions (`showParen`, `lex`, …) come from the module of the class or
//! the Prelude.

use crate::infer::{Tc, err};
use crate::types::*;
use achc_syntax::ast::{self, *};
use cirka_diagnostics::Span;

const DERIVABLE: &[&str] = &["Eq", "Ord", "Show", "Read", "Enum", "Bounded"];

struct Con {
    name: QName,
    arity: usize,
    fields: Option<Vec<String>>,
    /// Fixity when the constructor is an operator with two fields.
    infix: Option<u8>,
}

struct Gen {
    sp: Span,
    counter: u32,
}

impl Gen {
    fn local(&mut self, base: &str) -> String {
        self.counter += 1;
        format!("{base}#d{}", self.counter)
    }
    fn e(&self, kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: self.sp,
        }
    }
    fn p(&self, kind: PatKind) -> Pat {
        Pat {
            kind,
            span: self.sp,
        }
    }
    fn var(&self, q: &QName) -> Expr {
        self.e(ExprKind::Var(q.clone()))
    }
    fn lvar(&self, n: &str) -> Expr {
        self.e(ExprKind::Var(QName::unqual(n, self.sp)))
    }
    fn con(&self, q: &QName) -> Expr {
        self.e(ExprKind::Con(q.clone()))
    }
    fn app(&self, f: Expr, args: Vec<Expr>) -> Expr {
        args.into_iter()
            .fold(f, |f, a| self.e(ExprKind::App(Box::new(f), Box::new(a))))
    }
    fn int(&self, n: u128) -> Expr {
        self.e(ExprKind::Lit(Lit::Int(n)))
    }
    fn str(&self, s: &str) -> Expr {
        self.e(ExprKind::Lit(Lit::Str(s.into())))
    }
    fn pvar(&self, n: &str) -> Pat {
        self.p(PatKind::Var(Ident {
            name: n.into(),
            span: self.sp,
        }))
    }
    fn pcon(&self, q: &QName, ps: Vec<Pat>) -> Pat {
        self.p(PatKind::Con(q.clone(), ps))
    }
    fn plit_str(&self, s: &str) -> Pat {
        self.p(PatKind::Lit(Lit::Str(s.into())))
    }
    fn ptuple(&self, ps: Vec<Pat>) -> Pat {
        self.p(PatKind::Tuple(ps))
    }
    fn plain(&self, e: Expr) -> Rhs {
        Rhs {
            body: RhsBody::Plain(e),
            wheres: Vec::new(),
        }
    }
    fn clause(&self, method: &QName, pats: Vec<Pat>, body: Expr) -> Decl {
        Decl::Fun(
            Ident {
                name: method.text(),
                span: self.sp,
            },
            pats,
            self.plain(body),
            self.sp,
        )
    }
}

impl Tc {
    /// A global used by derived code: in the class's module, the Prelude
    /// or `Cirka.Prim`.
    fn helper(&mut self, class_mod: &str, name: &str, sp: Span) -> QName {
        for m in [class_mod, achc_rename::BASE_MODULE, "Prelude", "Cirka.Prim"] {
            let full = format!("{m}.{name}");
            if self.globals.contains_key(&full)
                || self.cons.contains_key(&full)
                || self.top_names.contains(&full)
            {
                return QName {
                    qual: Some(m.to_string()),
                    name: name.to_string(),
                    span: sp,
                };
            }
        }
        self.error(err(
            "E0201",
            sp,
            format!("deriving needs `{name}`, which is not defined"),
        ));
        QName {
            qual: Some("Prelude".into()),
            name: name.into(),
            span: sp,
        }
    }

    /// Instance declarations for the `deriving` clauses of all modules.
    pub fn derive_instances(&mut self, modules: &[(String, Vec<Decl>)]) -> Vec<(String, Decl)> {
        for (m, decls) in modules {
            for d in decls {
                match d {
                    Decl::Fixity(assoc, prec, ops, _) => {
                        for o in ops {
                            self.con_fixity
                                .insert(format!("{m}.{}", o.name), (*assoc, *prec));
                        }
                    }
                    Decl::TypeSig(ns, _, _) => {
                        for n in ns {
                            self.top_names.insert(format!("{m}.{}", n.name));
                        }
                    }
                    Decl::Fun(n, ..) => {
                        self.top_names.insert(format!("{m}.{}", n.name));
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        for (m, decls) in modules {
            for d in decls {
                let Decl::Data(dd) = d else { continue };
                for cls in &dd.deriving {
                    let class = cls.text();
                    let short_name = short(&class).to_string();
                    if !self.classes.contains_key(&class)
                        || !DERIVABLE.contains(&short_name.as_str())
                    {
                        self.error(
                            err(
                                "E0615",
                                cls.span,
                                format!("cannot derive instances of `{}`", short(&class)),
                            )
                            .with_note("derivable classes: Eq, Ord, Show, Read, Enum, Bounded"),
                        );
                        continue;
                    }
                    if let Some(i) = self.derive_one(m, dd, &class, &short_name) {
                        out.push((m.clone(), Decl::Instance(i)));
                    }
                }
            }
        }
        out
    }

    fn derive_one(
        &mut self,
        m: &str,
        dd: &ast::DataDecl,
        class: &str,
        which: &str,
    ) -> Option<InstDecl> {
        let sp = dd.span;
        let cm = self.classes[class].module.clone();
        let data = format!("{m}.{}", dd.name.name);
        let kinds = self.datas.get(&data)?.kinds.clone();
        let cons: Vec<Con> = dd
            .cons
            .iter()
            .map(|c| {
                let full = format!("{m}.{}", c.name.name);
                let (arity, fields) = match &c.fields {
                    ConFields::Plain(fs) => (fs.len(), None),
                    ConFields::Record(fs) => {
                        let names: Vec<String> = fs
                            .iter()
                            .flat_map(|(ns, _, _)| ns.iter().map(|n| n.name.clone()))
                            .collect();
                        (names.len(), Some(names))
                    }
                };
                let infix = (is_op_name(&c.name.name) && arity == 2 && fields.is_none())
                    .then(|| self.con_fixity.get(&full).map(|f| f.1).unwrap_or(9));
                Con {
                    name: QName {
                        qual: Some(m.to_string()),
                        name: c.name.name.clone(),
                        span: sp,
                    },
                    arity,
                    fields,
                    infix,
                }
            })
            .collect();
        let enumeration = !cons.is_empty() && cons.iter().all(|c| c.arity == 0);
        match which {
            "Enum" if !enumeration => {
                self.error(err(
                    "E0615",
                    sp,
                    format!(
                        "cannot derive `Enum` for `{}`: every constructor must be nullary",
                        dd.name.name
                    ),
                ));
                return None;
            }
            "Bounded" if !(enumeration || cons.len() == 1) => {
                self.error(err(
                    "E0615",
                    sp,
                    format!(
                        "cannot derive `Bounded` for `{}`: it must be an enumeration or have one constructor",
                        dd.name.name
                    ),
                ));
                return None;
            }
            _ => {}
        }
        let mut g = Gen { sp, counter: 0 };
        let method = |name: &str| QName {
            qual: Some(cm.clone()),
            name: name.into(),
            span: sp,
        };
        let decls = match which {
            "Eq" => self.derive_eq(&mut g, &cm, &cons, &method("==")),
            "Ord" => self.derive_ord(&mut g, &cm, &cons, &method("compare")),
            "Show" => self.derive_show(&mut g, &cm, &cons, &method("showsPrec")),
            "Read" => self.derive_read(&mut g, &cm, &cons, &method("readsPrec")),
            "Enum" => self.derive_enum(&mut g, &cm, &cons),
            "Bounded" => self.derive_bounded(&mut g, &cm, &cons),
            _ => return None,
        };
        // `instance (C a, ..) => C (T a ..)` for the parameters of kind *.
        let tvar = |n: &str| Type {
            kind: TypeKind::Var(n.into()),
            span: sp,
        };
        let head = dd.params.iter().fold(
            Type {
                kind: TypeKind::Con(QName {
                    qual: Some(m.to_string()),
                    name: dd.name.name.clone(),
                    span: sp,
                }),
                span: sp,
            },
            |f, p| Type {
                kind: TypeKind::App(Box::new(f), Box::new(tvar(&p.name))),
                span: sp,
            },
        );
        let cq = QName {
            qual: Some(cm.clone()),
            name: short(class).to_string(),
            span: sp,
        };
        let context = dd
            .params
            .iter()
            .zip(kinds)
            .filter(|(_, k)| *k == Kind::Star)
            .map(|(p, _)| Constraint {
                class: cq.clone(),
                types: vec![tvar(&p.name)],
            })
            .collect();
        Some(InstDecl {
            context,
            class: cq,
            types: vec![head],
            decls,
            span: sp,
        })
    }

    fn derive_eq(&mut self, g: &mut Gen, cm: &str, cons: &[Con], eq: &QName) -> Vec<Decl> {
        let and = self.helper(cm, "&&", g.sp);
        let tru = self.helper(cm, "True", g.sp);
        let fls = self.helper(cm, "False", g.sp);
        let mut out = Vec::new();
        for c in cons {
            let xs: Vec<String> = (0..c.arity).map(|_| g.local("a")).collect();
            let ys: Vec<String> = (0..c.arity).map(|_| g.local("b")).collect();
            let mut body: Option<Expr> = None;
            for (x, y) in xs.iter().zip(&ys).rev() {
                let t = g.app(g.var(eq), vec![g.lvar(x), g.lvar(y)]);
                body = Some(match body {
                    None => t,
                    Some(r) => g.app(g.var(&and), vec![t, r]),
                });
            }
            let pats = vec![
                g.pcon(&c.name, xs.iter().map(|x| g.pvar(x)).collect()),
                g.pcon(&c.name, ys.iter().map(|y| g.pvar(y)).collect()),
            ];
            out.push(g.clause(eq, pats, body.unwrap_or_else(|| g.con(&tru))));
        }
        if cons.len() != 1 {
            let res = if cons.is_empty() { &tru } else { &fls };
            let w = || g.p(PatKind::Wild);
            out.push(g.clause(eq, vec![w(), w()], g.con(res)));
        }
        out
    }

    fn derive_ord(&mut self, g: &mut Gen, cm: &str, cons: &[Con], cmp: &QName) -> Vec<Decl> {
        let lt = self.helper(cm, "LT", g.sp);
        let eqc = self.helper(cm, "EQ", g.sp);
        let gt = self.helper(cm, "GT", g.sp);
        let mut out = Vec::new();
        for c in cons {
            let xs: Vec<String> = (0..c.arity).map(|_| g.local("a")).collect();
            let ys: Vec<String> = (0..c.arity).map(|_| g.local("b")).collect();
            // case compare x1 y1 of { LT -> LT; GT -> GT; EQ -> ... }
            let mut body = g.con(&eqc);
            for (x, y) in xs.iter().zip(&ys).rev() {
                let scrut = g.app(g.var(cmp), vec![g.lvar(x), g.lvar(y)]);
                let alt = |p: &QName, e: Expr| Alt {
                    pat: g.pcon(p, Vec::new()),
                    rhs: g.plain(e),
                    span: g.sp,
                };
                body = g.e(ExprKind::Case(
                    Box::new(scrut),
                    vec![alt(&lt, g.con(&lt)), alt(&gt, g.con(&gt)), alt(&eqc, body)],
                ));
            }
            let pats = vec![
                g.pcon(&c.name, xs.iter().map(|x| g.pvar(x)).collect()),
                g.pcon(&c.name, ys.iter().map(|y| g.pvar(y)).collect()),
            ];
            out.push(g.clause(cmp, pats, body));
        }
        if cons.len() > 1 {
            // compare x y = compare (tag x) (tag y)
            let tag = |g: &mut Gen, v: &str| -> Expr {
                let alts = cons
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Alt {
                        pat: g.p(PatKind::Record(c.name.clone(), Vec::new())),
                        rhs: g.plain(g.int(i as u128)),
                        span: g.sp,
                    })
                    .collect();
                g.e(ExprKind::Case(Box::new(g.lvar(v)), alts))
            };
            let (x, y) = (g.local("x"), g.local("y"));
            let tx = tag(g, &x);
            let ty = tag(g, &y);
            let int = Type {
                kind: TypeKind::Con(QName {
                    qual: Some("Cirka.Prim".into()),
                    name: "Int".into(),
                    span: g.sp,
                }),
                span: g.sp,
            };
            let tx = g.e(ExprKind::Typed(Box::new(tx), int));
            out.push(g.clause(
                cmp,
                vec![g.pvar(&x), g.pvar(&y)],
                g.app(g.var(cmp), vec![tx, ty]),
            ));
        } else if cons.is_empty() {
            let w = || g.p(PatKind::Wild);
            out.push(g.clause(cmp, vec![w(), w()], g.con(&eqc)));
        }
        out
    }

    fn derive_show(&mut self, g: &mut Gen, cm: &str, cons: &[Con], sp_m: &QName) -> Vec<Decl> {
        let show_paren = self.helper(cm, "showParen", g.sp);
        let show_string = self.helper(cm, "showString", g.sp);
        let compose = self.helper(cm, ".", g.sp);
        let ge = self.helper(cm, ">=", g.sp);
        let gtq = self.helper(cm, ">", g.sp);
        let mut out = Vec::new();
        let seq = |g: &Gen, parts: Vec<Expr>| -> Expr {
            let mut it = parts.into_iter().rev();
            let last = it.next().unwrap();
            it.fold(last, |acc, p| g.app(g.var(&compose), vec![p, acc]))
        };
        for c in cons {
            let d = g.local("d");
            let xs: Vec<String> = (0..c.arity).map(|_| g.local("a")).collect();
            let name = &c.name.name;
            let shown = |n: &str| {
                if is_op_name(n) {
                    format!("({n})")
                } else {
                    n.to_string()
                }
            };
            let sstr = |g: &Gen, s: &str| g.app(g.var(&show_string), vec![g.str(s)]);
            let body = if let Some(p) = c.infix {
                let prec =
                    |g: &Gen, x: &str| g.app(g.var(sp_m), vec![g.int(p as u128 + 1), g.lvar(x)]);
                let inner = seq(
                    g,
                    vec![
                        prec(g, &xs[0]),
                        sstr(g, &format!(" {name} ")),
                        prec(g, &xs[1]),
                    ],
                );
                let cond = g.app(g.var(&gtq), vec![g.lvar(&d), g.int(p as u128)]);
                g.app(g.var(&show_paren), vec![cond, inner])
            } else if c.arity == 0 {
                sstr(g, &shown(name))
            } else if let Some(fs) = &c.fields {
                let mut parts = vec![sstr(g, &format!("{} {{", shown(name)))];
                for (i, (f, x)) in fs.iter().zip(&xs).enumerate() {
                    let sep = if i == 0 { "" } else { ", " };
                    parts.push(sstr(g, &format!("{sep}{} = ", shown(f))));
                    parts.push(g.app(g.var(sp_m), vec![g.int(0), g.lvar(x)]));
                }
                parts.push(sstr(g, "}"));
                let cond = g.app(g.var(&ge), vec![g.lvar(&d), g.int(11)]);
                g.app(g.var(&show_paren), vec![cond, seq(g, parts)])
            } else {
                let mut parts = vec![sstr(g, &format!("{} ", shown(name)))];
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        parts.push(sstr(g, " "));
                    }
                    parts.push(g.app(g.var(sp_m), vec![g.int(11), g.lvar(x)]));
                }
                let cond = g.app(g.var(&ge), vec![g.lvar(&d), g.int(11)]);
                g.app(g.var(&show_paren), vec![cond, seq(g, parts)])
            };
            let pats = vec![
                g.pvar(&d),
                g.pcon(&c.name, xs.iter().map(|x| g.pvar(x)).collect()),
            ];
            out.push(g.clause(sp_m, pats, body));
        }
        if cons.is_empty() {
            let w = || g.p(PatKind::Wild);
            let err_q = self.helper(cm, "error", g.sp);
            out.push(g.clause(
                sp_m,
                vec![w(), w()],
                g.app(g.var(&err_q), vec![g.str("Void showsPrec")]),
            ));
        }
        out
    }

    fn derive_read(&mut self, g: &mut Gen, cm: &str, cons: &[Con], rp: &QName) -> Vec<Decl> {
        let read_paren = self.helper(cm, "readParen", g.sp);
        let lex = self.helper(cm, "lex", g.sp);
        let gtq = self.helper(cm, ">", g.sp);
        let append = self.helper(cm, "++", g.sp);
        let fls = self.helper(cm, "False", g.sp);
        let d = g.local("d");
        let r = g.local("r");
        let mut alts: Vec<Expr> = Vec::new();
        for c in cons {
            let r0 = g.local("r");
            let mut quals = Vec::new();
            let mut cur = r0.clone();
            let lex_tok = |g: &mut Gen, quals: &mut Vec<Stmt>, cur: &mut String, tok: &str| {
                let nx = g.local("r");
                quals.push(Stmt::Bind(
                    g.ptuple(vec![g.plit_str(tok), g.pvar(&nx)]),
                    g.app(g.var(&lex), vec![g.lvar(cur)]),
                    g.sp,
                ));
                *cur = nx;
            };
            let read_arg = |g: &mut Gen, quals: &mut Vec<Stmt>, cur: &mut String, prec: u128| {
                let x = g.local("a");
                let nx = g.local("r");
                quals.push(Stmt::Bind(
                    g.ptuple(vec![g.pvar(&x), g.pvar(&nx)]),
                    g.app(g.var(rp), vec![g.int(prec), g.lvar(cur)]),
                    g.sp,
                ));
                *cur = nx;
                x
            };
            let mut xs = Vec::new();
            let paren_cond;
            if let Some(p) = c.infix {
                xs.push(read_arg(g, &mut quals, &mut cur, p as u128 + 1));
                lex_tok(g, &mut quals, &mut cur, &c.name.name);
                xs.push(read_arg(g, &mut quals, &mut cur, p as u128 + 1));
                paren_cond = g.app(g.var(&gtq), vec![g.lvar(&d), g.int(p as u128)]);
            } else {
                if is_op_name(&c.name.name) {
                    lex_tok(g, &mut quals, &mut cur, "(");
                    lex_tok(g, &mut quals, &mut cur, &c.name.name);
                    lex_tok(g, &mut quals, &mut cur, ")");
                } else {
                    lex_tok(g, &mut quals, &mut cur, &c.name.name);
                }
                if let Some(fs) = &c.fields {
                    lex_tok(g, &mut quals, &mut cur, "{");
                    for (i, f) in fs.iter().enumerate() {
                        if i > 0 {
                            lex_tok(g, &mut quals, &mut cur, ",");
                        }
                        lex_tok(g, &mut quals, &mut cur, f);
                        lex_tok(g, &mut quals, &mut cur, "=");
                        xs.push(read_arg(g, &mut quals, &mut cur, 0));
                    }
                    lex_tok(g, &mut quals, &mut cur, "}");
                    paren_cond = g.app(g.var(&gtq), vec![g.lvar(&d), g.int(11)]);
                } else {
                    for _ in 0..c.arity {
                        xs.push(read_arg(g, &mut quals, &mut cur, 11));
                    }
                    paren_cond = if c.arity == 0 {
                        g.con(&fls)
                    } else {
                        g.app(g.var(&gtq), vec![g.lvar(&d), g.int(10)])
                    };
                }
            }
            let value = g.app(g.con(&c.name), xs.iter().map(|x| g.lvar(x)).collect());
            let head = g.e(ExprKind::Tuple(vec![value, g.lvar(&cur)]));
            let comp = g.e(ExprKind::ListComp(Box::new(head), quals));
            let lam = g.e(ExprKind::Lam(vec![g.pvar(&r0)], Box::new(comp)));
            alts.push(g.app(g.var(&read_paren), vec![paren_cond, lam, g.lvar(&r)]));
        }
        let body = match alts.len() {
            0 => g.e(ExprKind::List(Vec::new())),
            _ => {
                let mut it = alts.into_iter().rev();
                let last = it.next().unwrap();
                it.fold(last, |acc, a| g.app(g.var(&append), vec![a, acc]))
            }
        };
        vec![g.clause(rp, vec![g.pvar(&d), g.pvar(&r)], body)]
    }

    fn derive_enum(&mut self, g: &mut Gen, cm: &str, cons: &[Con]) -> Vec<Decl> {
        let m = |n: &str| QName {
            qual: Some(cm.to_string()),
            name: n.into(),
            span: g.sp,
        };
        let from_enum = m("fromEnum");
        let to_enum = m("toEnum");
        let enum_from = m("enumFrom");
        let enum_from_then = m("enumFromThen");
        let enum_from_to = self.helper(cm, "enumFromTo", g.sp);
        let enum_from_then_to = self.helper(cm, "enumFromThenTo", g.sp);
        let ge = self.helper(cm, ">=", g.sp);
        let err_q = self.helper(cm, "error", g.sp);
        let mut out = Vec::new();
        for (i, c) in cons.iter().enumerate() {
            out.push(g.clause(
                &from_enum,
                vec![g.pcon(&c.name, Vec::new())],
                g.int(i as u128),
            ));
        }
        for (i, c) in cons.iter().enumerate() {
            out.push(g.clause(
                &to_enum,
                vec![g.p(PatKind::Lit(Lit::Int(i as u128)))],
                g.con(&c.name),
            ));
        }
        out.push(g.clause(
            &to_enum,
            vec![g.p(PatKind::Wild)],
            g.app(g.var(&err_q), vec![g.str("toEnum: bad argument")]),
        ));
        let first = &cons[0].name;
        let last = &cons[cons.len() - 1].name;
        let x = g.local("x");
        out.push(g.clause(
            &enum_from,
            vec![g.pvar(&x)],
            g.app(g.var(&enum_from_to), vec![g.lvar(&x), g.con(last)]),
        ));
        let (x, y) = (g.local("x"), g.local("y"));
        let up = g.app(
            g.var(&ge),
            vec![
                g.app(g.var(&from_enum), vec![g.lvar(&y)]),
                g.app(g.var(&from_enum), vec![g.lvar(&x)]),
            ],
        );
        let lim = g.e(ExprKind::If(
            Box::new(up),
            Box::new(g.con(last)),
            Box::new(g.con(first)),
        ));
        out.push(g.clause(
            &enum_from_then,
            vec![g.pvar(&x), g.pvar(&y)],
            g.app(g.var(&enum_from_then_to), vec![g.lvar(&x), g.lvar(&y), lim]),
        ));
        out
    }

    fn derive_bounded(&mut self, g: &mut Gen, cm: &str, cons: &[Con]) -> Vec<Decl> {
        let m = |n: &str| QName {
            qual: Some(cm.to_string()),
            name: n.into(),
            span: g.sp,
        };
        let (minb, maxb) = (m("minBound"), m("maxBound"));
        if cons.iter().all(|c| c.arity == 0) {
            return vec![
                g.clause(&minb, Vec::new(), g.con(&cons[0].name)),
                g.clause(&maxb, Vec::new(), g.con(&cons[cons.len() - 1].name)),
            ];
        }
        let c = &cons[0];
        let mk = |b: &QName| g.app(g.con(&c.name), (0..c.arity).map(|_| g.var(b)).collect());
        vec![
            g.clause(&minb, Vec::new(), mk(&minb)),
            g.clause(&maxb, Vec::new(), mk(&maxb)),
        ]
    }
}
