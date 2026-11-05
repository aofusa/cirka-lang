//! Type classes (FEAT-0110): class and instance declarations, dictionary
//! types, constraint solving and defaulting.
//!
//! A class `C a` is a data type `C a` with one constructor `C$Dict` whose
//! fields are the superclass dictionaries and then the methods (a method
//! with its own type variables has a `forall` field). A method is a
//! selector `forall a. C a -> τ`; an instance is a dictionary function
//! `$fC.. : forall vs. ctx => C (T vs)` built from the instance methods
//! `$cC..`, which are type checked like bindings with a signature.
//!
//! Overloaded expressions get dictionary holes (`tast::Expr::Hole`). At
//! generalization the constraints of a group are reduced to head normal
//! form (`C a` for a type variable `a`): those on generalized variables
//! become dictionary parameters, the others go to the enclosing scope.
//! The top level defaults ambiguous numeric variables (Haskell 2010
//! §4.3.4: `Integer`, then `Double`).

use crate::infer::{ClassInfo, InstInfo, Tc, Wanted, err};
use crate::tast::{self, BindGroup, Clause, Rhs};
use crate::types::*;
use achc_syntax::ast::{self, Decl};
use cirka_diagnostics::Span;
use std::collections::{HashMap, HashSet};

/// Classes whose ambiguous variables may be defaulted.
const NUMERIC: &[&str] = &[
    "Num",
    "Real",
    "Integral",
    "Fractional",
    "Floating",
    "RealFrac",
    "RealFloat",
];

/// Whether `class` is one of the standard numeric classes.
fn is_numeric(class: &str) -> bool {
    let (m, s) = class.rsplit_once('.').unwrap_or(("", class));
    (m == "Prelude" || m == achc_rename::BASE_MODULE) && NUMERIC.contains(&s)
}

/// `e` wrapped in a lambda per dictionary; the type gains the arrows.
pub(crate) fn wrap_dicts(dicts: &[(String, Ty)], e: tast::Expr, t: Ty) -> (tast::Expr, Ty) {
    let mut e = e;
    let mut t = t;
    for (d, dt) in dicts.iter().rev() {
        e = tast::Expr::Lam(d.clone(), dt.clone(), Box::new(e));
        t = Ty::fun(dt.clone(), t);
    }
    (e, t)
}

pub(crate) fn app_all(f: tast::Expr, args: impl IntoIterator<Item = tast::Expr>) -> tast::Expr {
    args.into_iter()
        .fold(f, |f, a| tast::Expr::App(Box::new(f), Box::new(a)))
}

/// One-way matching of an instance head (variables `vars`) against `t`.
fn match_ty(pat: &Ty, t: &Ty, vars: &[String], sub: &mut Vec<(String, Ty)>) -> bool {
    match (pat, t) {
        (Ty::Var(v), _) if vars.contains(v) => match sub.iter().find(|(n, _)| n == v) {
            Some((_, b)) => b == t,
            None => {
                sub.push((v.clone(), t.clone()));
                true
            }
        },
        (Ty::Var(a), Ty::Var(b)) => a == b,
        (Ty::Con(a), Ty::Con(b)) => a == b,
        (Ty::App(f, a), Ty::App(g, b)) => match_ty(f, g, vars, sub) && match_ty(a, b, vars, sub),
        (Ty::Fun(a, b), Ty::Fun(c, d)) => match_ty(a, c, vars, sub) && match_ty(b, d, vars, sub),
        // `(->) r` against a function type.
        (Ty::App(f, a), Ty::Fun(c, d)) => {
            let arrow = Ty::App(Box::new(Ty::Con(prim("->"))), c.clone());
            match_ty(f, &arrow, vars, sub) && match_ty(a, d, vars, sub)
        }
        _ => false,
    }
}

impl Tc {
    // ---- declarations ----

    /// First pass: the class name and its variable.
    pub(crate) fn declare_class_name(&mut self, m: &str, c: &ast::ClassDecl) {
        let name = format!("{m}.{}", c.name.name);
        // A class without parameters (NullaryTypeClasses) has a packed
        // constraint type with no arguments.
        let vars: Vec<String> = c
            .params
            .iter()
            .map(|p| self.fresh_name(&format!("{}#", p.name)))
            .collect();
        let kinds = vec![Kind::Star; vars.len()];
        self.add_data(&name, vars.clone(), kinds.clone(), false);
        self.classes.insert(
            name,
            ClassInfo {
                module: m.to_string(),
                vars,
                kinds,
                supers: Vec::new(),
                methods: Vec::new(),
                defaults: HashSet::new(),
            },
        );
    }

    /// Superclasses, methods, the dictionary type and the selectors.
    /// Returns the default methods as value declarations.
    pub fn declare_classes(
        &mut self,
        modules: &[(String, Vec<Decl>)],
    ) -> (Vec<(String, Decl)>, Vec<BindGroup>) {
        let mut values = Vec::new();
        let mut order = Vec::new();
        for (m, decls) in modules {
            for d in decls {
                let Decl::Class(c) = d else { continue };
                let name = format!("{m}.{}", c.name.name);
                let Some(info) = self.classes.get(&name).cloned() else {
                    continue;
                };
                let src_vars: Vec<String> = c.params.iter().map(|p| p.name.clone()).collect();
                let src_var = src_vars.first().cloned().unwrap_or_default();
                let nv = info.vars.len();
                let mut supers = Vec::new();
                for k in &c.context {
                    let sc = k.class.text();
                    let ok = nv == 1
                        && k.types.len() == 1
                        && matches!(&k.types[0].kind, ast::TypeKind::Var(v) if *v == src_var);
                    if !ok {
                        self.error(err(
                            "E0001",
                            k.class.span,
                            "a superclass constraint must be on the class variable",
                        ));
                    } else if !self.classes.contains_key(&sc) {
                        self.error(err(
                            "E0201",
                            k.class.span,
                            format!("class `{}` is not defined", short(&sc)),
                        ));
                    } else {
                        supers.push(sc);
                    }
                }
                let mut methods = Vec::new();
                let mut defaults = HashSet::new();
                for d in &c.decls {
                    match d {
                        Decl::TypeSig(ns, t, _) => {
                            let mut tvs: Vec<(String, Ty)> = src_vars
                                .iter()
                                .cloned()
                                .zip(info.vars.iter().map(|v| Ty::Var(v.clone())))
                                .collect();
                            let saved = std::mem::take(&mut self.qual_preds);
                            let (ty, _) = self.ast_sig_ty(t, &mut tvs);
                            let extra = std::mem::replace(&mut self.qual_preds, saved);
                            let mut vars = info.vars.clone();
                            vars.extend(tvs[nv..].iter().filter_map(|(_, t)| match t {
                                Ty::Var(v) => Some(v.clone()),
                                _ => None,
                            }));
                            let mut preds = vec![Pred::new(&name, info.class_ty())];
                            preds.extend(extra);
                            for n in ns {
                                let mname = format!("{m}.{}", n.name);
                                let s = Scheme {
                                    vars: vars.clone(),
                                    preds: preds.clone(),
                                    ty: ty.clone(),
                                };
                                self.globals.insert(mname.clone(), s.clone());
                                methods.push((mname, s));
                            }
                        }
                        Decl::Fun(f, args, rhs, sp) => {
                            let mname = format!("{m}.{}", f.name);
                            if !defaults.contains(&mname) {
                                defaults.insert(mname.clone());
                            }
                            let dm = format!("$dm{}", f.name);
                            values.push((
                                m.clone(),
                                Decl::Fun(
                                    ast::Ident {
                                        name: dm,
                                        span: f.span,
                                    },
                                    args.clone(),
                                    rhs.clone(),
                                    *sp,
                                ),
                            ));
                        }
                        _ => {}
                    }
                }
                for (mname, s) in &methods {
                    if defaults.contains(mname) {
                        let dm = format!("{m}.$dm{}", short(mname));
                        self.extra_sigs.insert(dm, s.clone());
                    }
                }
                for dm in &defaults {
                    if !methods.iter().any(|(n, _)| n == dm) {
                        self.error(err(
                            "E0201",
                            c.span,
                            format!(
                                "`{}` is not a method of class `{}`",
                                short(dm),
                                short(&name)
                            ),
                        ));
                    }
                }
                // The kinds of the class variables from the method types.
                let mut kinds = vec![Kind::Star; nv];
                for (i, (v, kind)) in info.vars.iter().zip(kinds.iter_mut()).enumerate() {
                    if let Some(Some(k)) = c.param_kinds.get(i) {
                        *kind = Kind::from_sig(k);
                        if *k == achc_syntax::ast::KindSig::LType {
                            self.ltype_vars.insert(v.clone());
                            self.has_ltype = true;
                        }
                        continue;
                    }
                    for (_, s) in &methods {
                        if let Some(n) = crate::infer::applied_arity(v, &s.ty) {
                            *kind = Kind::arity(n);
                        }
                    }
                }
                let ci = self.classes.get_mut(&name).unwrap();
                ci.supers = supers;
                ci.methods = methods;
                ci.defaults = defaults;
                ci.kinds = kinds.clone();
                self.datas.get_mut(&name).unwrap().kinds = kinds;
                order.push(name);
            }
        }
        // Dictionary types and selectors.
        let mut binds = Vec::new();
        for name in order {
            let ci = self.classes[&name].clone();
            let cv = ci.class_ty();
            let cvs: Vec<Ty> = ci.vars.iter().map(|v| Ty::Var(v.clone())).collect();
            let ctvs: Vec<(String, Kind)> = ci
                .vars
                .iter()
                .cloned()
                .zip(ci.kinds.iter().cloned())
                .collect();
            let mut fields: Vec<Ty> = ci
                .supers
                .iter()
                .map(|s| Pred::new(s, cv.clone()).dict_ty())
                .collect();
            for (_, s) in &ci.methods {
                fields.push(method_field(s, ci.vars.len()));
            }
            let con = format!("{name}$Dict");
            let n = fields.len();
            self.add_con(&con, &name, fields.clone(), vec![false; n]);
            self.out.datas.push(tast::DataDecl {
                name: name.clone(),
                params: ctvs.clone(),
                cons: vec![tast::ConDecl {
                    name: con.clone(),
                    fields: fields.clone(),
                    strict: vec![false; n],
                }],
                newtype: false,
            });
            let dty = Pred::new(&name, cv.clone()).dict_ty();
            let nsup = ci.supers.len();
            for (i, ft) in fields.iter().enumerate() {
                let (sel, mut tvs, res, sel_ty) = if i < nsup {
                    let sel = format!("{name}$super{i}");
                    (sel, ctvs.clone(), ft.clone(), None)
                } else {
                    let (mname, s) = &ci.methods[i - nsup];
                    (mname.clone(), ctvs.clone(), ft.clone(), Some(s))
                };
                let f = self.fresh_name("$m");
                let d = self.fresh_name("$dict");
                // The field instantiated at the selector's own variables.
                let (body, res) = match (&res, sel_ty) {
                    (Ty::Forall(vs, inner), Some(_)) => {
                        for v in vs {
                            tvs.push((v.clone(), self.kind_in(v, inner)));
                        }
                        let inst = vs.iter().map(|v| Ty::Var(v.clone())).collect();
                        (tast::Expr::Global(f.clone(), inst), (**inner).clone())
                    }
                    _ => (tast::Expr::Local(f.clone(), res.clone()), res.clone()),
                };
                let pats = fields
                    .iter()
                    .enumerate()
                    .map(|(k, t)| {
                        if k == i {
                            tast::Pat::Var(f.clone(), t.clone())
                        } else {
                            tast::Pat::Wild(t.clone())
                        }
                    })
                    .collect();
                let m = tast::Expr::Match(
                    vec![(d.clone(), dty.clone())],
                    vec![Clause {
                        pats: vec![tast::Pat::Con(con.clone(), cvs.clone(), pats)],
                        rhs: Rhs {
                            wheres: Vec::new(),
                            body: vec![(Vec::new(), body)],
                        },
                        span: Span::DUMMY,
                    }],
                    res.clone(),
                );
                if i < nsup {
                    self.globals.insert(
                        sel.clone(),
                        Scheme::poly(ci.vars.clone(), Ty::fun(dty.clone(), res.clone())),
                    );
                }
                binds.push(BindGroup::NonRec(tast::Bind {
                    name: sel,
                    tvs,
                    ty: Ty::fun(dty.clone(), res),
                    rhs: tast::Expr::Lam(d, dty.clone(), Box::new(m)),
                    span: Span::DUMMY,
                    strict: false,
                }));
            }
        }
        (values, binds)
    }

    /// Instance declarations (including derived ones): the instance
    /// methods as value declarations with schemes, and the dictionary
    /// functions.
    pub fn declare_instances(
        &mut self,
        modules: &[(String, Vec<Decl>)],
    ) -> (Vec<(String, Decl)>, Vec<BindGroup>) {
        let mut pending = Vec::new();
        for (m, decls) in modules {
            for d in decls {
                let Decl::Instance(i) = d else { continue };
                if let Some(idx) = self.register_instance(m, i) {
                    pending.push((m.clone(), idx, i.decls.clone()));
                }
            }
        }
        let mut values = Vec::new();
        let mut binds = Vec::new();
        for (m, idx, decls) in pending {
            let inst = self.instances[idx].clone();
            let ci = self.classes[&inst.class].clone();
            let csub = ci.csub(&inst.head);
            let nv = ci.vars.len();
            let mut impls: HashMap<String, String> = HashMap::new();
            for d in decls {
                let Decl::Fun(f, args, rhs, sp) = d else {
                    continue;
                };
                let Some((_, s)) = ci.methods.iter().find(|(n, _)| *n == f.name) else {
                    self.error(err(
                        "E0201",
                        f.span,
                        format!(
                            "`{}` is not a method of class `{}`",
                            short(&f.name),
                            short(&inst.class)
                        ),
                    ));
                    continue;
                };
                let local = match impls.get(&f.name) {
                    Some(l) => l.clone(),
                    None => {
                        let l = format!("$c{}{idx}${}", short(&inst.class), short(&f.name));
                        impls.insert(f.name.clone(), l.clone());
                        let mut vars = inst.vars.clone();
                        vars.extend(s.vars[nv..].iter().cloned());
                        let mut preds = inst.ctx.clone();
                        preds.extend(s.preds[1..].iter().map(|p| p.subst(&csub)));
                        self.extra_sigs.insert(
                            format!("{m}.{l}"),
                            Scheme {
                                vars,
                                preds,
                                ty: s.ty.subst(&csub),
                            },
                        );
                        l
                    }
                };
                values.push((
                    m.clone(),
                    Decl::Fun(
                        ast::Ident {
                            name: local,
                            span: f.span,
                        },
                        args,
                        rhs,
                        sp,
                    ),
                ));
            }
            let impls: HashMap<String, String> = impls
                .into_iter()
                .map(|(k, v)| (k, format!("{m}.{v}")))
                .collect();
            binds.push(BindGroup::NonRec(self.dfun_bind(&inst, &ci, &impls)));
        }
        (values, binds)
    }

    fn register_instance(&mut self, m: &str, i: &ast::InstDecl) -> Option<usize> {
        let class = i.class.text();
        if !self.classes.contains_key(&class) {
            self.error(err(
                "E0201",
                i.class.span,
                format!("class `{}` is not defined", short(&class)),
            ));
            return None;
        }
        let nv = self.classes[&class].vars.len();
        if i.types.len() != nv {
            self.error(err(
                "E0001",
                i.span,
                format!(
                    "the class `{}` takes {nv} type argument{}, but the instance has {}",
                    short(&class),
                    if nv == 1 { "" } else { "s" },
                    i.types.len()
                ),
            ));
            return None;
        }
        let mut tvs = Vec::new();
        let saved = std::mem::take(&mut self.qual_preds);
        let head_tys: Vec<Ty> = i
            .types
            .iter()
            .map(|t| self.ast_ty(t, &mut tvs, true))
            .collect();
        let head = crate::infer::pack_class_tys(head_tys);
        let mut ctx = Vec::new();
        for c in &i.context {
            if let Some(p) = self.constraint(c, &mut tvs, false) {
                ctx.push(p);
            }
        }
        self.qual_preds = saved;
        if nv == 1 && head.head_con().is_none() {
            self.error(
                err(
                    "E0001",
                    i.types[0].span,
                    format!(
                        "the instance head `{}` must be a type constructor application",
                        show(&head)
                    ),
                )
                .with_help("write the instance for a data type, e.g. `instance C (Maybe a)`"),
            );
            return None;
        }
        let vars: Vec<String> = tvs
            .into_iter()
            .filter_map(|(_, t)| match t {
                Ty::Var(v) => Some(v),
                _ => None,
            })
            .collect();
        for other in &self.instances {
            if other.class != class {
                continue;
            }
            let mut s1 = Vec::new();
            let mut s2 = Vec::new();
            if match_ty(&other.head, &head, &other.vars, &mut s1)
                || match_ty(&head, &other.head, &vars, &mut s2)
            {
                self.error(
                    err(
                        "E0612",
                        i.span,
                        format!(
                            "duplicate or overlapping instance `{} {}`",
                            short(&class),
                            show(&head)
                        ),
                    )
                    .with_secondary(other.span, "the other instance is here"),
                );
                return None;
            }
        }
        let idx = self.instances.len();
        let dfun = format!("{m}.$f{}{idx}", short(&class));
        self.globals.insert(
            dfun.clone(),
            Scheme {
                vars: vars.clone(),
                preds: ctx.clone(),
                ty: Pred::new(&class, head.clone()).dict_ty(),
            },
        );
        self.instances.push(InstInfo {
            class,
            vars,
            head,
            ctx,
            dfun,
            span: i.span,
        });
        Some(idx)
    }

    /// `$fC.. = Λvs. λctx. C$Dict supers.. methods..`
    fn dfun_bind(
        &mut self,
        inst: &InstInfo,
        ci: &ClassInfo,
        impls: &HashMap<String, String>,
    ) -> tast::Bind {
        let csub = ci.csub(&inst.head);
        let nv = ci.vars.len();
        let head_args = ci.head_args(&inst.head);
        let dicts = self.dict_params(&inst.ctx);
        let gstart = self.givens.len();
        for (p, (d, t)) in inst.ctx.iter().zip(&dicts) {
            self.givens
                .push((p.clone(), tast::Expr::Local(d.clone(), t.clone())));
        }
        let vars_ty: Vec<Ty> = inst.vars.iter().map(|v| Ty::Var(v.clone())).collect();
        let ctx_args: Vec<tast::Expr> = dicts
            .iter()
            .map(|(d, t)| tast::Expr::Local(d.clone(), t.clone()))
            .collect();
        let self_dict = app_all(
            tast::Expr::Global(inst.dfun.clone(), vars_ty.clone()),
            ctx_args.clone(),
        );
        let mut fields = Vec::new();
        for s in &ci.supers {
            let p = Pred::new(s, inst.head.clone());
            match self.solve(&p, &|_, _| true) {
                Ok(e) => fields.push(e),
                Err(_) => {
                    self.error(
                        err(
                            "E0608",
                            inst.span,
                            format!(
                                "no instance for `{}` arising from the superclasses of an instance declaration",
                                show_pred(&p)
                            ),
                        )
                        .with_help("declare the superclass instance as well"),
                    );
                    fields.push(tast::Expr::Error("superclass".into(), p.dict_ty()));
                }
            }
        }
        let mut missing = Vec::new();
        for (mname, s) in &ci.methods {
            let extra_vars = &s.vars[nv..];
            let extra_preds: Vec<Pred> = s.preds[1..].iter().map(|p| p.subst(&csub)).collect();
            let ty = s.ty.subst(&csub);
            let impl_expr = if let Some(g) = impls.get(mname) {
                let mut inst_tys = vars_ty.clone();
                inst_tys.extend(extra_vars.iter().map(|v| Ty::Var(v.clone())));
                Some(app_all(
                    tast::Expr::Global(g.clone(), inst_tys),
                    ctx_args.clone(),
                ))
            } else if ci.defaults.contains(mname) {
                let dm = format!("{}.$dm{}", ci.module, short(mname));
                let mut inst_tys = head_args.clone();
                inst_tys.extend(extra_vars.iter().map(|v| Ty::Var(v.clone())));
                Some(app_all(
                    tast::Expr::Global(dm, inst_tys),
                    [self_dict.clone()],
                ))
            } else {
                missing.push(short(mname).to_string());
                None
            };
            let eds = self.dict_params(&extra_preds);
            let body = match impl_expr {
                Some(e) => app_all(
                    e,
                    eds.iter()
                        .map(|(d, t)| tast::Expr::Local(d.clone(), t.clone())),
                ),
                None => tast::Expr::Error(
                    format!(
                        "No instance nor default method for class operation {}",
                        short(mname)
                    ),
                    ty.clone(),
                ),
            };
            let (lam, lty) = wrap_dicts(&eds, body, ty);
            if extra_vars.is_empty() {
                fields.push(lam);
            } else {
                let m = self.fresh_name("$meth");
                let tvs = extra_vars
                    .iter()
                    .map(|v| (v.clone(), self.kind_in(v, &lty)))
                    .collect();
                let fty = Ty::Forall(extra_vars.to_vec(), Box::new(lty.clone()));
                fields.push(tast::Expr::Let(
                    vec![BindGroup::NonRec(tast::Bind {
                        name: m.clone(),
                        tvs,
                        ty: lty,
                        rhs: lam,
                        span: Span::DUMMY,
                        strict: false,
                    })],
                    Box::new(tast::Expr::Local(m, fty)),
                ));
            }
        }
        self.givens.truncate(gstart);
        if !missing.is_empty() {
            self.out.warnings.push(
                cirka_diagnostics::Diagnostic::warning(format!(
                    "no explicit implementation for {} in the instance `{} {}`",
                    missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    short(&inst.class),
                    show(&inst.head)
                ))
                .with_code("W0613")
                .with_label(inst.span, "")
                .with_note("calling a missing method is a runtime error"),
            );
        }
        let con = format!("{}$Dict", inst.class);
        let body = app_all(tast::Expr::Con(con, head_args), fields);
        let dty = Pred::new(&inst.class, inst.head.clone()).dict_ty();
        let (rhs, ty) = wrap_dicts(&dicts, body, dty);
        let tvs = inst
            .vars
            .iter()
            .map(|v| (v.clone(), self.kind_in(v, &ty)))
            .collect();
        tast::Bind {
            name: inst.dfun.clone(),
            tvs,
            ty,
            rhs,
            span: inst.span,
            strict: false,
        }
    }

    /// Instances derived for newtypes (GeneralizedNewtypeDeriving): the
    /// dictionary of the representation type, coerced. The class's last
    /// parameter may have a higher kind: the newtype's last parameters
    /// are then dropped from both sides (`newtype P a = P (Maybe a)
    /// deriving Functor` uses `Functor Maybe`).
    pub fn declare_newtype_instances(&mut self) -> Vec<BindGroup> {
        let pending = std::mem::take(&mut self.gnd_pending);
        let mut todo = Vec::new();
        for (m, data, class, span) in pending {
            let ci = self.classes[&class].clone();
            let cannot = |why: String| {
                err(
                    "E0615",
                    span,
                    format!(
                        "cannot derive `{}` for the newtype `{}`: {why}",
                        short(&class),
                        short(&data)
                    ),
                )
            };
            if ci.vars.len() != 1 {
                self.error(cannot("the class has several parameters".into()));
                continue;
            }
            let k = ci.kinds[0].args();
            let di = self.datas[&data].clone();
            let field = self.cons[&di.cons[0]].fields[0].clone();
            let n = di.params.len();
            let Some(keep) = n.checked_sub(k) else {
                self.error(cannot("the newtype has too few parameters".into()));
                continue;
            };
            let Some(rep) = eta_reduce(&field, &di.params[keep..]) else {
                self.error(cannot(format!(
                    "its last parameter{} must be the last argument{} of the representation type",
                    if k == 1 { "" } else { "s" },
                    if k == 1 { "" } else { "s" },
                )));
                continue;
            };
            let vars: Vec<String> = di.params[..keep]
                .iter()
                .map(|v| self.fresh_name(&format!("{}#", short(v).trim_end_matches('#'))))
                .collect();
            let sub: Vec<(String, Ty)> = di.params[..keep]
                .iter()
                .cloned()
                .zip(vars.iter().map(|v| Ty::Var(v.clone())))
                .collect();
            let rep = rep.subst(&sub);
            let head = Ty::con(&data, vars.iter().map(|v| Ty::Var(v.clone())).collect());
            let mut ctx = Vec::new();
            let has_vars = |_: &Tc, t: &Ty| {
                let mut vs = Vec::new();
                t.vars(&mut vs);
                !vs.is_empty()
            };
            if let Err(p) = self.hnf(&Pred::new(&class, rep.clone()), &has_vars, &mut ctx) {
                self.error(
                    err(
                        "E0608",
                        span,
                        format!(
                            "no instance for `{}` arising from deriving `{}` for a newtype",
                            show_pred(&p),
                            short(&class)
                        ),
                    )
                    .with_help("the representation type must have the instance"),
                );
                continue;
            }
            if self.instance_for(&class, &head).is_some() {
                self.error(err(
                    "E0612",
                    span,
                    format!(
                        "duplicate or overlapping instance `{} {}`",
                        short(&class),
                        show(&head)
                    ),
                ));
                continue;
            }
            let idx = self.instances.len();
            let dfun = format!("{m}.$f{}{idx}", short(&class));
            let p = Pred::new(&class, head.clone());
            self.globals.insert(
                dfun.clone(),
                Scheme {
                    vars: vars.clone(),
                    preds: ctx.clone(),
                    ty: p.dict_ty(),
                },
            );
            self.instances.push(InstInfo {
                class: class.clone(),
                vars,
                head,
                ctx,
                dfun,
                span,
            });
            todo.push((idx, rep));
        }
        let mut binds = Vec::new();
        for (idx, rep) in todo {
            let inst = self.instances[idx].clone();
            let dicts = self.dict_params(&inst.ctx);
            let gstart = self.givens.len();
            for (p, (d, t)) in inst.ctx.iter().zip(&dicts) {
                self.givens
                    .push((p.clone(), tast::Expr::Local(d.clone(), t.clone())));
            }
            let rp = Pred::new(&inst.class, rep);
            let e = match self.solve(&rp, &|_, _| true) {
                Ok(e) => e,
                Err(p) => {
                    self.no_instance(&p, inst.span);
                    tast::Expr::Error("newtype instance".into(), p.dict_ty())
                }
            };
            self.givens.truncate(gstart);
            let dty = Pred::new(&inst.class, inst.head.clone()).dict_ty();
            let body = tast::Expr::Coerce(Box::new(e), dty.clone());
            let (rhs, ty) = wrap_dicts(&dicts, body, dty);
            let tvs = inst
                .vars
                .iter()
                .map(|v| (v.clone(), self.kind_in(v, &ty)))
                .collect();
            binds.push(BindGroup::NonRec(tast::Bind {
                name: inst.dfun.clone(),
                tvs,
                ty,
                rhs,
                span: inst.span,
                strict: false,
            }));
        }
        binds
    }

    // ---- kinds ----

    /// The kind of type variable `v` in `t`: from an application of `v`,
    /// or from its position as the argument of a data type or class.
    pub(crate) fn kind_in(&self, v: &str, t: &Ty) -> Kind {
        if let Some(n) = crate::infer::applied_arity(v, t) {
            return Kind::arity(n);
        }
        self.positional_kind(v, t).unwrap_or(Kind::Star)
    }

    fn positional_kind(&self, v: &str, t: &Ty) -> Option<Kind> {
        match t {
            Ty::App(..) => {
                let (h, args) = t.spine();
                if let Ty::Con(c) = h
                    && let Some(d) = self.datas.get(c)
                {
                    for (i, a) in args.iter().enumerate() {
                        if matches!(a, Ty::Var(x) if x == v)
                            && let Some(k) = d.kinds.get(i)
                        {
                            return Some(k.clone());
                        }
                    }
                }
                args.iter().find_map(|a| self.positional_kind(v, a))
            }
            Ty::Fun(a, b) => self
                .positional_kind(v, a)
                .or_else(|| self.positional_kind(v, b)),
            Ty::Forall(_, b) => self.positional_kind(v, b),
            _ => None,
        }
    }

    // ---- solving ----

    /// Dictionary parameter names and types for a context.
    pub(crate) fn dict_params(&mut self, ctx: &[Pred]) -> Vec<(String, Ty)> {
        ctx.iter()
            .map(|p| (self.fresh_name("$dict"), p.dict_ty()))
            .collect()
    }

    /// The superclass path from class `from` to class `to`.
    fn super_path(&self, from: &str, to: &str) -> Option<Vec<(String, usize)>> {
        if from == to {
            return Some(Vec::new());
        }
        let ci = self.classes.get(from)?;
        for (i, s) in ci.supers.iter().enumerate() {
            if let Some(mut p) = self.super_path(s, to) {
                p.insert(0, (from.to_string(), i));
                return Some(p);
            }
        }
        None
    }

    fn instance_for(&self, class: &str, t: &Ty) -> Option<(usize, Vec<(String, Ty)>)> {
        self.instances.iter().enumerate().find_map(|(i, inst)| {
            if inst.class != class {
                return None;
            }
            let mut sub = Vec::new();
            match_ty(&inst.head, t, &inst.vars, &mut sub).then_some((i, sub))
        })
    }

    /// A dictionary for `p` from the givens and the instances. A
    /// constraint on a type variable that is not `local` goes to the
    /// enclosing scope as a new hole. `Err` is the unsolvable constraint.
    pub(crate) fn solve(
        &mut self,
        p: &Pred,
        local: &dyn Fn(&Tc, &Ty) -> bool,
    ) -> Result<tast::Expr, Pred> {
        let t = self.zonk(&p.ty);
        for k in (0..self.givens.len()).rev() {
            let (g, e) = self.givens[k].clone();
            if self.zonk(&g.ty) != t {
                continue;
            }
            if let Some(path) = self.super_path(&g.class, &p.class) {
                let mut e = e;
                for (c, i) in path {
                    e = tast::Expr::App(
                        Box::new(tast::Expr::Global(format!("{c}$super{i}"), vec![t.clone()])),
                        Box::new(e),
                    );
                }
                return Ok(e);
            }
        }
        let found = if t.head_con().is_some() {
            self.instance_for(&p.class, &t)
        } else {
            None
        };
        if found.is_none() && t.head_con().is_some() && !open_mp(&t) {
            return Err(Pred::new(&p.class, t));
        }
        if let Some((i, sub)) = found {
            let inst = self.instances[i].clone();
            let tys: Vec<Ty> = inst
                .vars
                .iter()
                .map(|v| {
                    sub.iter()
                        .find(|(n, _)| n == v)
                        .map(|x| x.1.clone())
                        .unwrap_or_else(|| Ty::Con(unit_tc()))
                })
                .collect();
            let mut args = Vec::new();
            for c in &inst.ctx {
                args.push(self.solve(&c.subst(&sub), local)?);
            }
            return Ok(app_all(tast::Expr::Global(inst.dfun.clone(), tys), args));
        }
        if !local(self, &t) {
            let h = self.want(Pred::new(&p.class, t), Span::DUMMY);
            return Ok(tast::Expr::Hole(h));
        }
        Err(Pred::new(&p.class, t))
    }

    /// Head normal form of `p`: the constraints on `local` type variables
    /// it needs (instances applied). `Err` is a constraint without
    /// instance.
    fn hnf(
        &mut self,
        p: &Pred,
        local: &dyn Fn(&Tc, &Ty) -> bool,
        out: &mut Vec<Pred>,
    ) -> Result<(), Pred> {
        let t = self.zonk(&p.ty);
        // Already given (e.g. the context of an enclosing signature).
        for (g, _) in self.givens.clone() {
            if self.zonk(&g.ty) == t && self.super_path(&g.class, &p.class).is_some() {
                return Ok(());
            }
        }
        let found = if t.head_con().is_some() {
            self.instance_for(&p.class, &t)
        } else {
            None
        };
        if found.is_none() && t.head_con().is_some() && !open_mp(&t) {
            return Err(Pred::new(&p.class, t));
        }
        if let Some((i, sub)) = found {
            for c in self.instances[i].ctx.clone() {
                self.hnf(&c.subst(&sub), local, out)?;
            }
            return Ok(());
        }
        if local(self, &t) {
            let q = Pred::new(&p.class, t);
            if !out.contains(&q) {
                out.push(q);
            }
        }
        Ok(())
    }

    fn no_instance(&mut self, p: &Pred, span: Span) {
        let mut d = err("E0608", span, format!("no instance for `{}`", show_pred(p)));
        d = match &p.ty {
            Ty::Var(v) => d.with_help(format!(
                "add `{} {}` to the context of the type signature",
                short(&p.class),
                achc_rename::display_name(v)
            )),
            Ty::Fun(..) => d.with_note("functions have no instances of this class"),
            _ => d.with_help("declare an instance, or derive it with a `deriving` clause"),
        };
        self.error(d);
    }

    fn ambiguous(&mut self, p: &Pred, span: Span) {
        self.error(
            err(
                "E0609",
                span,
                format!(
                    "ambiguous type variable in the constraint `{}`",
                    show_pred(p)
                ),
            )
            .with_help("add a type annotation that fixes the type (e.g. `:: Int`)"),
        );
    }

    /// Defaults the metas of `cands` that only appear as `C m` with a
    /// numeric class `C` (Haskell 2010 §4.3.4).
    fn default_metas(&mut self, cands: &[u32], preds: &[Pred]) {
        // Reduce through instances first: `Show [Maybe t]` constrains `t`
        // as `Show t`.
        let mut reduced = Vec::new();
        for p in preds {
            let mut out = Vec::new();
            match self.hnf(p, &|_, _| true, &mut out) {
                Ok(()) => reduced.extend(out),
                Err(q) => reduced.push(q),
            }
        }
        let preds = &reduced[..];
        for &m in cands {
            if self.shallow(&Ty::Meta(m)) != Ty::Meta(m) {
                continue;
            }
            let mut on = Vec::new();
            let mut simple = true;
            for p in preds {
                let t = self.zonk(&p.ty);
                let mut ms = Vec::new();
                self.metas_of(&t, &mut ms);
                if !ms.contains(&m) {
                    continue;
                }
                if t == Ty::Meta(m) {
                    on.push(p.class.clone());
                } else {
                    simple = false;
                }
            }
            if !simple || !on.iter().any(|c| is_numeric(c)) {
                continue;
            }
            for cand in [self.integer_ty(), Ty::Con(prim("Double"))] {
                let ok = on.iter().all(|c| {
                    let mut out = Vec::new();
                    self.hnf(&Pred::new(c, cand.clone()), &|_, _| true, &mut out)
                        .is_ok()
                        && out.is_empty()
                });
                if ok {
                    self.solve_meta(m, cand);
                    break;
                }
            }
        }
    }

    /// Splits the constraints of an inferred group: the context of the
    /// generalized bindings (on metas of `ms`) and the constraints whose
    /// holes the group solves. Others are deferred to the enclosing scope.
    pub(crate) fn generalize_preds(
        &mut self,
        new: Vec<Wanted>,
        ms: &mut Vec<u32>,
        tys: &[Ty],
        restricted: bool,
    ) -> (Vec<Pred>, Vec<Wanted>) {
        let mut mine = Vec::new();
        for mut w in new {
            w.pred.ty = self.zonk(&w.pred.ty);
            let mut mm = Vec::new();
            self.metas_of(&w.pred.ty, &mut mm);
            if mm.iter().any(|m| ms.contains(m)) {
                mine.push(w);
            } else {
                self.wanted.push(w);
            }
        }
        if restricted {
            // Constrained variables stay monomorphic (and may be
            // defaulted at the top level).
            let level = self.level();
            for w in &mine {
                let mut mm = Vec::new();
                self.metas_of(&w.pred.ty, &mut mm);
                for m in mm {
                    self.set_meta_level(m, level);
                    ms.retain(|x| *x != m);
                }
            }
            self.wanted.extend(mine);
            return (Vec::new(), Vec::new());
        }
        // Ambiguous variables: constrained but not in the types.
        let mut in_tys = Vec::new();
        for t in tys {
            self.metas_of(t, &mut in_tys);
        }
        let mut amb = Vec::new();
        for w in &mine {
            let mut mm = Vec::new();
            self.metas_of(&w.pred.ty, &mut mm);
            for m in mm {
                if ms.contains(&m) && !in_tys.contains(&m) && !amb.contains(&m) {
                    amb.push(m);
                }
            }
        }
        if !amb.is_empty() {
            let preds: Vec<Pred> = mine.iter().map(|w| w.pred.clone()).collect();
            self.default_metas(&amb, &preds);
            ms.retain(|m| self.shallow(&Ty::Meta(*m)) == Ty::Meta(*m));
        }
        let gms: Vec<u32> = ms.clone();
        let local = move |tc: &Tc, t: &Ty| {
            let mut mm = Vec::new();
            tc.metas_of(t, &mut mm);
            mm.iter().any(|m| gms.contains(m))
        };
        let mut ctx = Vec::new();
        let mut ok = Vec::new();
        for w in mine {
            match self.hnf(&w.pred, &local, &mut ctx) {
                Ok(()) => ok.push(w),
                Err(p) => {
                    let ground = {
                        let mut mm = Vec::new();
                        self.metas_of(&p.ty, &mut mm);
                        mm.is_empty()
                    };
                    if ground || (p.ty.head_con().is_some() && !open_mp(&p.ty)) {
                        self.no_instance(&p, w.span);
                    } else {
                        self.ambiguous(&p, w.span);
                    }
                }
            }
        }
        // Constraints implied by a superclass of another are dropped. A
        // variable that is still ambiguous is not generalized: its
        // constraints go to the enclosing scope (and are reported at the
        // top level).
        let level = self.level();
        let mut kept: Vec<Pred> = Vec::new();
        for p in &ctx {
            let implied = ctx.iter().any(|q| {
                q != p
                    && q.ty == p.ty
                    && q.class != p.class
                    && self.super_path(&q.class, &p.class).is_some()
            });
            if implied {
                continue;
            }
            let mut mm = Vec::new();
            self.metas_of(&p.ty, &mut mm);
            if mm.iter().any(|m| !in_tys.contains(m)) {
                for m in mm {
                    if !in_tys.contains(&m) {
                        self.set_meta_level(m, level);
                        ms.retain(|x| *x != m);
                    }
                }
                continue;
            }
            kept.push(p.clone());
        }
        (kept, ok)
    }

    /// Solves the holes of `ws` with the dictionary parameters `dicts`
    /// (for the context `ctx` given by their types).
    pub(crate) fn solve_with(&mut self, ws: Vec<Wanted>, dicts: &[(String, Ty)]) {
        let gstart = self.givens.len();
        for (d, t) in dicts {
            let (h, args) = t.spine();
            if let Ty::Con(class) = h
                && !args.is_empty()
            {
                let tys = args.into_iter().cloned().collect();
                self.givens.push((
                    Pred::new(class, crate::infer::pack_class_tys(tys)),
                    tast::Expr::Local(d.clone(), t.clone()),
                ));
            }
        }
        for w in ws {
            match self.solve(&w.pred, &|tc, t| {
                // Only generalized variables (now rigid) are local here.
                let mut vs = Vec::new();
                tc.zonk(t).vars(&mut vs);
                !vs.is_empty()
            }) {
                Ok(e) => {
                    self.dict_sol.insert(w.hole, e);
                }
                Err(p) => self.report_unsolved(&p, w.span),
            }
        }
        self.givens.truncate(gstart);
    }

    /// The constraints of a binding with a signature: those on its own
    /// type variables (`skolems`) or on metas created inside it are solved
    /// from the givens; others are deferred.
    pub(crate) fn solve_sig(&mut self, new: Vec<Wanted>, skolems: &[String]) {
        let level = self.level();
        let mut must = Vec::new();
        for mut w in new {
            w.pred.ty = self.zonk(&w.pred.ty);
            if self.is_local_sig(&w.pred.ty, skolems, level) {
                must.push(w);
            } else {
                self.wanted.push(w);
            }
        }
        // Inner metas that remain are ambiguous: default them.
        let mut cands = Vec::new();
        for w in &must {
            let mut mm = Vec::new();
            self.metas_of(&w.pred.ty, &mut mm);
            for m in mm {
                if self.meta_level(m) > level && !cands.contains(&m) {
                    cands.push(m);
                }
            }
        }
        let preds: Vec<Pred> = must.iter().map(|w| w.pred.clone()).collect();
        self.default_metas(&cands, &preds);
        let sk = skolems.to_vec();
        for w in must {
            let local = |tc: &Tc, t: &Ty| tc.is_local_sig(t, &sk, level);
            match self.solve(&w.pred, &local) {
                Ok(e) => {
                    self.dict_sol.insert(w.hole, e);
                }
                Err(p) => self.report_unsolved(&p, w.span),
            }
        }
    }

    fn is_local_sig(&self, t: &Ty, skolems: &[String], level: u32) -> bool {
        let t = self.zonk(t);
        let mut vs = Vec::new();
        t.vars(&mut vs);
        if vs.iter().any(|v| skolems.contains(v)) {
            return true;
        }
        let mut mm = Vec::new();
        self.metas_of(&t, &mut mm);
        mm.iter().any(|m| self.meta_level(*m) > level)
    }

    fn report_unsolved(&mut self, p: &Pred, span: Span) {
        let mut mm = Vec::new();
        self.metas_of(&p.ty, &mut mm);
        if !mm.is_empty() && (p.ty.head_con().is_none() || open_mp(&p.ty)) {
            self.ambiguous(p, span);
        } else {
            self.no_instance(p, span);
        }
    }

    /// The constraints left at the top level: defaulting, then instances.
    pub fn solve_top(&mut self) {
        let ws = std::mem::take(&mut self.wanted);
        let mut cands = Vec::new();
        let mut preds = Vec::new();
        for w in &ws {
            let t = self.zonk(&w.pred.ty);
            self.metas_of(&t, &mut cands);
            preds.push(Pred::new(&w.pred.class, t));
        }
        self.default_metas(&cands, &preds);
        for w in ws {
            match self.solve(&w.pred, &|_, _| true) {
                Ok(e) => {
                    self.dict_sol.insert(w.hole, e);
                }
                Err(p) => self.report_unsolved(&p, w.span),
            }
        }
        // Holes created while solving (none are deferred at the top).
        let rest = std::mem::take(&mut self.wanted);
        for w in rest {
            self.report_unsolved(&w.pred, w.span);
        }
    }
}

/// `t` without its last arguments `vs` (which must not occur elsewhere).
fn eta_reduce(t: &Ty, vs: &[String]) -> Option<Ty> {
    let mut t = t.clone();
    for v in vs.iter().rev() {
        let Ty::App(f, a) = &t else { return None };
        if !matches!(&**a, Ty::Var(x) if x == v) {
            return None;
        }
        t = (**f).clone();
    }
    let mut free = Vec::new();
    t.vars(&mut free);
    if free.iter().any(|x| vs.contains(x)) {
        return None;
    }
    Some(t)
}

/// A multi-parameter constraint with a type variable or meta in an
/// argument and no matching instance yet: like a constraint on a variable,
/// it is deferred or taken from the context instead of being an error.
fn open_mp(t: &Ty) -> bool {
    match crate::types::mp_args(t) {
        Some(args) => args.iter().any(|a| {
            let mut vs = Vec::new();
            a.vars(&mut vs);
            !vs.is_empty() || has_meta(a)
        }),
        None => false,
    }
}

fn has_meta(t: &Ty) -> bool {
    match t {
        Ty::Meta(_) => true,
        Ty::Var(_) | Ty::Con(_) => false,
        Ty::App(a, b) | Ty::Fun(a, b) => has_meta(a) || has_meta(b),
        Ty::Forall(_, b) => has_meta(b),
    }
}

/// The dictionary field of a method: `forall extra. ctx -> τ`.
fn method_field(s: &Scheme, nv: usize) -> Ty {
    let extra_preds = &s.preds[1..];
    let body = extra_preds
        .iter()
        .rev()
        .fold(s.ty.clone(), |t, p| Ty::fun(p.dict_ty(), t));
    if s.vars.len() > nv {
        Ty::Forall(s.vars[nv..].to_vec(), Box::new(body))
    } else {
        body
    }
}
