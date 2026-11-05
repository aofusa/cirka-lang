//! Hindley–Milner inference with let-polymorphism over the renamed AST,
//! elaborating it into the typed program (`tast`).

use crate::classes::{app_all, wrap_dicts};
use crate::tast::{self, BindGroup, Clause, Guard, Rhs};
use crate::types::*;
use achc_syntax::ast::{self, ConFields, Decl, ExprKind, PatKind, Stmt, TypeKind};
use cirka_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

pub(crate) fn err(code: &str, span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg).with_code(code).with_label(span, "")
}

#[derive(Clone, Debug)]
pub struct DataInfo {
    pub params: Vec<String>,
    pub kinds: Vec<Kind>,
    pub cons: Vec<String>,
    pub newtype: bool,
}

#[derive(Clone, Debug)]
pub struct ConInfo {
    pub data: String,
    /// Field types over the data type's parameters (`Ty::Var`).
    pub fields: Vec<Ty>,
    pub strict: Vec<bool>,
    pub field_names: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
struct Meta {
    sol: Option<Ty>,
    level: u32,
}

#[derive(Clone, Debug)]
enum LocalKind {
    /// Lambda- or pattern-bound: monomorphic.
    Mono(Ty),
    /// Let-bound: a scheme (monomorphic while its group is inferred).
    Let(Scheme),
}

/// Marker for references to a binding of the group being inferred; replaced
/// by the group's type variables after generalization.
pub(crate) const REC: &str = "#rec";

/// A class constraint to solve; its dictionary fills `hole`.
#[derive(Clone, Debug)]
pub(crate) struct Wanted {
    pub pred: Pred,
    pub hole: u32,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub module: String,
    /// The class variables (unique type variable names); more than one
    /// for a multi-parameter class.
    pub vars: Vec<String>,
    pub kinds: Vec<Kind>,
    pub supers: Vec<String>,
    /// Methods (global names) with their schemes: the class variable and
    /// the class constraint first, then the method's own.
    pub methods: Vec<(String, Scheme)>,
    /// Methods with a default definition.
    pub defaults: HashSet<String>,
}

impl ClassInfo {
    /// The type of a constraint on the class variables: the variable, or
    /// the packed variables of a multi-parameter class.
    pub fn class_ty(&self) -> Ty {
        let vs: Vec<Ty> = self.vars.iter().map(|v| Ty::Var(v.clone())).collect();
        pack_class_tys(vs)
    }

    /// The class arguments of a constraint type (`[t]` or the packed ones).
    pub fn head_args(&self, t: &Ty) -> Vec<Ty> {
        if self.vars.len() == 1 {
            return vec![t.clone()];
        }
        match crate::types::mp_args(t) {
            Some(a) => a.into_iter().cloned().collect(),
            None => vec![t.clone()],
        }
    }

    /// The substitution of the class variables by a constraint type.
    pub fn csub(&self, t: &Ty) -> Vec<(String, Ty)> {
        self.vars.iter().cloned().zip(self.head_args(t)).collect()
    }
}

/// The type of a class constraint on `tys` (packed when there are several).
pub fn pack_class_tys(mut tys: Vec<Ty>) -> Ty {
    if tys.len() == 1 {
        tys.pop().unwrap()
    } else {
        Ty::con(&crate::types::mp_tc(tys.len()), tys)
    }
}

#[derive(Clone, Debug)]
pub struct InstInfo {
    pub class: String,
    pub vars: Vec<String>,
    pub head: Ty,
    pub ctx: Vec<Pred>,
    /// The dictionary function.
    pub dfun: String,
    pub span: Span,
}

pub struct Tc {
    metas: Vec<Meta>,
    level: u32,
    pub datas: HashMap<String, DataInfo>,
    pub cons: HashMap<String, ConInfo>,
    synonyms: HashMap<String, (Vec<String>, ast::Type)>,
    syn_module: HashMap<String, String>,
    pub globals: HashMap<String, Scheme>,
    locals: HashMap<String, LocalKind>,
    /// Bindings of groups being inferred, to their quantified variables.
    rec_tvs: HashMap<String, Vec<String>>,
    /// Local bindings evaluated before the body (`let !p = e`).
    strict_locals: HashSet<String>,
    fields: HashMap<String, Vec<(String, usize)>>,
    pub(crate) counter: u32,
    pub errors: Vec<Diagnostic>,
    pub out: tast::Program,
    pub classes: HashMap<String, ClassInfo>,
    pub instances: Vec<InstInfo>,
    pub(crate) wanted: Vec<Wanted>,
    /// Dictionaries in scope (signature contexts, instance contexts).
    pub(crate) givens: Vec<(Pred, tast::Expr)>,
    /// Solutions of the dictionary holes.
    pub(crate) dict_sol: HashMap<u32, tast::Expr>,
    holes: u32,
    /// Dictionary parameters of the bindings of a group, for recursive
    /// references (`REC`).
    rec_dicts: HashMap<String, Vec<(String, Ty)>>,
    /// Constraints of the contexts met by `ast_ty`.
    pub(crate) qual_preds: Vec<Pred>,
    /// Schemes of generated bindings (default and instance methods).
    pub(crate) extra_sigs: HashMap<String, Scheme>,
    /// Fixity of constructor operators (derived `Show` / `Read`).
    pub(crate) con_fixity: HashMap<String, (ast::Assoc, u8)>,
    /// Newtype-derived instances to declare (module, data type, class,
    /// span of the `deriving` entry).
    pub(crate) gnd_pending: Vec<(String, String, String, Span)>,
    /// Type variables in scope (ScopedTypeVariables): those bound by an
    /// explicit `forall` of the signatures of the enclosing bindings.
    scoped: Vec<(String, Ty)>,
    /// LinearTypes: the multiplicity of the arrow each argument (by the
    /// span of the argument expression) is passed to, and of the arrow each
    /// parameter pattern (by its span) is bound by, when it is not `Many`.
    pub(crate) lin_args: HashMap<Span, Ty>,
    pub(crate) lin_pats: HashMap<Span, Ty>,
    /// Uniqueness (FEAT-0121, 03-type-system.md §2.2). Set when the
    /// program declares linear-only types or `LType` type variables; the
    /// records below are kept only then.
    pub(crate) has_ltype: bool,
    /// Data types declared `type T :: k1 -> .. -> LType` (with the arity).
    pub(crate) ltype_datas: HashMap<String, usize>,
    /// Type variables (unique names) declared `:: LType`.
    pub(crate) ltype_vars: HashSet<String>,
    /// The type of each pattern and of each let-bound name (by span).
    pub(crate) pat_tys: HashMap<Span, Ty>,
    /// Arguments passed to unrestricted arrows (U-Bind, E0301).
    pub(crate) omega_args: Vec<(Span, Ty)>,
    /// The type of each argument (by span).
    pub(crate) arg_tys: HashMap<Span, Ty>,
    /// Instantiations of `Type` variables of a referenced name (E0310):
    /// (use site, name, variable, its instance).
    pub(crate) inst_checks: Vec<(Span, String, String, Ty)>,
    /// Metas created by those instantiations (E0310 reports them, not
    /// E0301).
    inst_metas: HashSet<u32>,
    /// The type of each `do` block (by span): `IO` blocks run their
    /// statements once.
    pub(crate) do_tys: HashMap<Span, Ty>,
    /// Top-level value names of all modules (for deriving helpers).
    pub(crate) top_names: HashSet<String>,
    /// Source files and their modules, for the call stack of `error`
    /// (`file:line:col in main:Module`).
    pub(crate) sources: Option<(cirka_diagnostics::SourceMap, HashMap<u32, String>)>,
}

impl Tc {
    pub fn new() -> Tc {
        let mut tc = Tc {
            metas: Vec::new(),
            level: 0,
            datas: HashMap::new(),
            cons: HashMap::new(),
            synonyms: HashMap::new(),
            syn_module: HashMap::new(),
            globals: HashMap::new(),
            locals: HashMap::new(),
            rec_tvs: HashMap::new(),
            strict_locals: HashSet::new(),
            fields: HashMap::new(),
            counter: 0,
            errors: Vec::new(),
            out: tast::Program::default(),
            classes: HashMap::new(),
            instances: Vec::new(),
            wanted: Vec::new(),
            givens: Vec::new(),
            dict_sol: HashMap::new(),
            holes: 0,
            rec_dicts: HashMap::new(),
            qual_preds: Vec::new(),
            extra_sigs: HashMap::new(),
            con_fixity: HashMap::new(),
            gnd_pending: Vec::new(),
            scoped: Vec::new(),
            lin_args: HashMap::new(),
            lin_pats: HashMap::new(),
            has_ltype: false,
            ltype_datas: HashMap::new(),
            ltype_vars: HashSet::new(),
            pat_tys: HashMap::new(),
            omega_args: Vec::new(),
            arg_tys: HashMap::new(),
            inst_checks: Vec::new(),
            inst_metas: HashSet::new(),
            do_tys: HashMap::new(),
            top_names: HashSet::new(),
            sources: None,
        };
        // Built-in syntax: lists and unit.
        let a = || Ty::Var("a".into());
        tc.add_data(&list_tc(), vec!["a".into()], vec![Kind::Star], false);
        tc.add_con(&list_tc(), &list_tc(), vec![], vec![]);
        tc.add_con(
            &prim(":"),
            &list_tc(),
            vec![a(), Ty::list(a())],
            vec![false, false],
        );
        tc.add_data(&unit_tc(), vec![], vec![], false);
        tc.add_con(&unit_tc(), &unit_tc(), vec![], vec![]);
        tc
    }

    pub(crate) fn add_data(
        &mut self,
        name: &str,
        params: Vec<String>,
        kinds: Vec<Kind>,
        newtype: bool,
    ) {
        self.datas.insert(
            name.to_string(),
            DataInfo {
                params,
                kinds,
                cons: Vec::new(),
                newtype,
            },
        );
    }

    pub(crate) fn add_con(&mut self, name: &str, data: &str, fields: Vec<Ty>, strict: Vec<bool>) {
        let n = fields.len();
        self.datas
            .get_mut(data)
            .unwrap()
            .cons
            .push(name.to_string());
        self.cons.insert(
            name.to_string(),
            ConInfo {
                data: data.to_string(),
                fields,
                strict,
                field_names: vec![None; n],
            },
        );
    }

    /// Tuple types are created on demand.
    fn ensure_tuple(&mut self, n: usize) -> String {
        let tc = tuple_tc(n);
        if !self.datas.contains_key(&tc) {
            let ps: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
            let fs: Vec<Ty> = ps.iter().map(|p| Ty::Var(p.clone())).collect();
            self.add_data(&tc, ps, vec![Kind::Star; n], false);
            self.add_con(&tc, &tc, fs, vec![false; n]);
        }
        tc
    }

    pub(crate) fn error(&mut self, d: Diagnostic) {
        self.errors.push(d);
    }

    /// A source span as GHC prints it in runtime errors: `f.hs:3:5-12`,
    /// or `f.hs:(3,5)-(4,2)` across lines.
    pub(crate) fn ghc_loc(&self, sp: Span) -> Option<String> {
        let (sm, _) = self.sources.as_ref()?;
        if sp.is_dummy() {
            return None;
        }
        let file = sm.get(sp.file)?;
        let (l1, c1) = file.line_col(sp.start);
        let (l2, c2) = file.line_col(sp.end.max(sp.start + 1) - 1);
        Some(if l1 != l2 {
            format!("{}:({l1},{c1})-({l2},{c2})", file.name)
        } else if c1 != c2 {
            format!("{}:{l1}:{c1}-{c2}", file.name)
        } else {
            format!("{}:{l1}:{c1}", file.name)
        })
    }

    /// `msg` prefixed with the GHC location of `sp` when it is known.
    fn located(&self, sp: Span, msg: &str) -> String {
        match self.ghc_loc(sp) {
            Some(l) => format!("{l}: {msg}"),
            None => msg.to_string(),
        }
    }

    pub(crate) fn fresh_name(&mut self, base: &str) -> String {
        self.counter += 1;
        format!("{base}{}", self.counter)
    }

    // ---- metas and unification ----

    pub fn fresh(&mut self) -> Ty {
        self.metas.push(Meta {
            sol: None,
            level: self.level,
        });
        Ty::Meta(self.metas.len() as u32 - 1)
    }

    pub(crate) fn shallow(&self, t: &Ty) -> Ty {
        let mut t = t.clone();
        while let Ty::Meta(m) = t {
            match &self.metas[m as usize].sol {
                Some(s) => t = s.clone(),
                None => break,
            }
        }
        t
    }

    pub fn zonk(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::App(f, a) => {
                let t = Ty::App(Box::new(self.zonk(&f)), Box::new(self.zonk(&a)));
                // `#FUN #Many a b` is `a -> b`.
                match as_mul_fun(&t) {
                    Some((m, a, b)) if *m == mult_many() => Ty::fun(a.clone(), b.clone()),
                    _ => t,
                }
            }
            Ty::Fun(a, b) => Ty::Fun(Box::new(self.zonk(&a)), Box::new(self.zonk(&b))),
            Ty::Forall(vs, b) => Ty::Forall(vs, Box::new(self.zonk(&b))),
            t => t,
        }
    }

    /// Metas of `t`.
    pub(crate) fn metas_of(&self, t: &Ty, out: &mut Vec<u32>) {
        match self.shallow(t) {
            Ty::Meta(m) => {
                if !out.contains(&m) {
                    out.push(m)
                }
            }
            Ty::App(a, b) | Ty::Fun(a, b) => {
                self.metas_of(&a, out);
                self.metas_of(&b, out);
            }
            Ty::Forall(_, b) => self.metas_of(&b, out),
            _ => {}
        }
    }

    pub(crate) fn meta_level(&self, m: u32) -> u32 {
        self.metas[m as usize].level
    }

    pub(crate) fn set_meta_level(&mut self, m: u32, l: u32) {
        let x = &mut self.metas[m as usize].level;
        if *x > l {
            *x = l;
        }
    }

    pub(crate) fn solve_meta(&mut self, m: u32, t: Ty) {
        self.metas[m as usize].sol = Some(t);
    }

    pub(crate) fn level(&self) -> u32 {
        self.level
    }

    /// A new dictionary hole for `p`.
    pub(crate) fn want(&mut self, p: Pred, span: Span) -> u32 {
        self.holes += 1;
        let h = self.holes;
        self.wanted.push(Wanted {
            pred: p,
            hole: h,
            span,
        });
        h
    }

    /// Occurs check and level adjustment before binding meta `m`.
    fn occurs(&mut self, m: u32, level: u32, t: &Ty) -> bool {
        match self.shallow(t) {
            Ty::Meta(n) => {
                if n == m {
                    return true;
                }
                let l = &mut self.metas[n as usize].level;
                if *l > level {
                    *l = level;
                }
                false
            }
            Ty::App(a, b) | Ty::Fun(a, b) => self.occurs(m, level, &a) || self.occurs(m, level, &b),
            Ty::Forall(_, b) => self.occurs(m, level, &b),
            _ => false,
        }
    }

    pub(crate) fn unify_raw(&mut self, a: &Ty, b: &Ty) -> Result<(), UErr> {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Ty::Meta(x), Ty::Meta(y)) if x == y => Ok(()),
            (Ty::Meta(m), t) | (t, Ty::Meta(m)) => {
                let level = self.metas[*m as usize].level;
                if self.occurs(*m, level, t) {
                    return Err(UErr::Occurs);
                }
                self.metas[*m as usize].sol = Some(t.clone());
                Ok(())
            }
            (Ty::Var(x), Ty::Var(y)) if x == y => Ok(()),
            (Ty::Con(x), Ty::Con(y)) if x == y => Ok(()),
            (Ty::App(f, x), Ty::App(g, y)) => {
                self.unify_raw(f, g)?;
                self.unify_raw(x, y)
            }
            (Ty::Fun(a1, b1), Ty::Fun(a2, b2)) => {
                self.unify_raw(a1, a2)?;
                self.unify_raw(b1, b2)
            }
            // `a -> b` is `FUN Many a b`.
            (Ty::Fun(a1, b1), t @ Ty::App(..)) | (t @ Ty::App(..), Ty::Fun(a1, b1))
                if as_mul_fun(t).is_some() =>
            {
                let (m, a2, b2) = as_mul_fun(t).unwrap();
                let (m, a2, b2) = (m.clone(), a2.clone(), b2.clone());
                self.unify_raw(&m, &mult_many())?;
                self.unify_raw(a1, &a2)?;
                self.unify_raw(b1, &b2)
            }
            // `m a ~ (r -> a)` with `m := (->) r`.
            (Ty::App(f, x), Ty::Fun(a1, b1)) | (Ty::Fun(a1, b1), Ty::App(f, x)) => {
                let arrow = Ty::App(Box::new(Ty::Con(prim("->"))), a1.clone());
                self.unify_raw(f, &arrow)?;
                self.unify_raw(x, b1)
            }
            (Ty::Var(v), _) | (_, Ty::Var(v)) => Err(UErr::Rigid(v.clone())),
            _ => Err(UErr::Mismatch),
        }
    }

    /// Unifies `expected` with `actual`, reporting at `span`.
    pub fn unify(&mut self, span: Span, expected: &Ty, actual: &Ty) -> bool {
        // A linear (or `%0`) function can be used where an unrestricted one
        // is expected (not under other type constructors: variance).
        if let Ty::Fun(ea, eb) = self.shallow(expected)
            && let a = self.shallow(actual)
            && let Some((m, aa, ab)) = as_mul_fun(&a)
            && matches!(self.shallow(m), Ty::Con(c) if is_mult_con(&c))
        {
            let (aa, ab) = (aa.clone(), ab.clone());
            if self.unify_raw(&ea, &aa).is_ok() && self.unify_raw(&eb, &ab).is_ok() {
                return true;
            }
        }
        match self.unify_raw(expected, actual) {
            Ok(()) => true,
            Err(e) => {
                let ex = show(&self.zonk(expected));
                let ac = show(&self.zonk(actual));
                let d = match e {
                    UErr::Mismatch => err(
                        "E0601",
                        span,
                        format!("couldn't match expected type `{ex}` with actual type `{ac}`"),
                    ),
                    UErr::Rigid(v) => err(
                        "E0604",
                        span,
                        format!("couldn't match expected type `{ex}` with actual type `{ac}`"),
                    )
                    .with_note(format!(
                        "`{}` is a rigid type variable bound by a type signature",
                        achc_rename::display_name(&v)
                    )),
                    UErr::Occurs => err(
                        "E0602",
                        span,
                        format!("cannot construct the infinite type `{ex} ~ {ac}`"),
                    )
                    .with_help("a value is used both as a whole and as a part of itself"),
                };
                self.error(d);
                false
            }
        }
    }

    pub fn instantiate(&mut self, s: &Scheme) -> (Ty, Vec<Ty>) {
        let (t, inst, _) = self.instantiate_q(s);
        (t, inst)
    }

    /// Instantiates a scheme with fresh metas, also its constraints.
    pub fn instantiate_q(&mut self, s: &Scheme) -> (Ty, Vec<Ty>, Vec<Pred>) {
        let inst: Vec<Ty> = s.vars.iter().map(|_| self.fresh()).collect();
        let sub: Vec<(String, Ty)> = s.vars.iter().cloned().zip(inst.iter().cloned()).collect();
        let preds = s.preds.iter().map(|p| p.subst(&sub)).collect();
        (s.ty.subst(&sub), inst, preds)
    }

    /// `name @inst` applied to a dictionary hole per constraint.
    fn global_ref(&mut self, name: &str, s: &Scheme, span: Span) -> (tast::Expr, Ty) {
        let (t, inst, preds) = self.instantiate_q(s);
        let mut e = tast::Expr::Global(name.to_string(), inst);
        for p in preds {
            let h = self.want(p, span);
            e = tast::Expr::App(Box::new(e), Box::new(tast::Expr::Hole(h)));
        }
        (e, t)
    }

    /// Metas of `t` above the current level.
    fn gen_metas(&self, t: &Ty, out: &mut Vec<u32>) {
        match self.shallow(t) {
            Ty::Meta(m) => {
                if self.metas[m as usize].level > self.level && !out.contains(&m) {
                    out.push(m);
                }
            }
            Ty::App(a, b) | Ty::Fun(a, b) => {
                self.gen_metas(&a, out);
                self.gen_metas(&b, out);
            }
            _ => {}
        }
    }

    fn lower_levels(&mut self, t: &Ty) {
        let level = self.level;
        let _ = self.occurs(u32::MAX, level, t);
    }

    // ---- types from the AST ----

    /// Converts a source type; `tvs` maps source type variables.
    pub fn ast_ty(&mut self, t: &ast::Type, tvs: &mut Vec<(String, Ty)>, bind_new: bool) -> Ty {
        match &t.kind {
            TypeKind::Var(v) => {
                if let Some((_, ty)) = tvs.iter().rev().find(|(n, _)| n == v) {
                    return ty.clone();
                }
                if bind_new {
                    let ty = Ty::Var(self.fresh_name(&format!("{v}#")));
                    tvs.push((v.clone(), ty.clone()));
                    ty
                } else {
                    self.error(err(
                        "E0201",
                        t.span,
                        format!("type variable `{v}` is not in scope"),
                    ));
                    self.fresh()
                }
            }
            TypeKind::Con(_) | TypeKind::App(..) => {
                let (h, args) = achc_syntax::parser::flatten_tyapp(t);
                let args: Vec<Ty> = args.iter().map(|a| self.ast_ty(a, tvs, bind_new)).collect();
                let TypeKind::Con(q) = &h.kind else {
                    let h = self.ast_ty(h, tvs, bind_new);
                    return args
                        .into_iter()
                        .fold(h, |f, a| Ty::App(Box::new(f), Box::new(a)));
                };
                let name = q.text();
                if let Some((ps, body)) = self.synonyms.get(&name).cloned() {
                    if args.len() < ps.len() {
                        self.error(err(
                            "E0603",
                            t.span,
                            format!(
                                "type synonym `{}` should have {} arguments, but has been given {}",
                                short(&name),
                                ps.len(),
                                args.len()
                            ),
                        ));
                        return self.fresh();
                    }
                    let mut sub: Vec<(String, Ty)> =
                        ps.iter().cloned().zip(args.iter().cloned()).collect();
                    let body = self.ast_ty(&body, &mut sub, false);
                    return args[ps.len()..]
                        .iter()
                        .cloned()
                        .fold(body, |f, a| Ty::App(Box::new(f), Box::new(a)));
                }
                if name == prim("->") && args.len() == 2 {
                    return Ty::fun(args[0].clone(), args[1].clone());
                }
                if let Some(n) = tuple_arity(&name) {
                    self.ensure_tuple(n);
                }
                if !self.datas.contains_key(&name) && name != prim("->") {
                    self.error(err(
                        "E0201",
                        q.span,
                        format!("type `{}` is not defined", short(&name)),
                    ));
                }
                Ty::con(&name, args)
            }
            TypeKind::Fun(a, b) => {
                let a = self.ast_ty(a, tvs, bind_new);
                let b = self.ast_ty(b, tvs, bind_new);
                Ty::fun(a, b)
            }
            TypeKind::List(a) => {
                let a = self.ast_ty(a, tvs, bind_new);
                Ty::list(a)
            }
            TypeKind::Tuple(ts) => {
                let args: Vec<Ty> = ts.iter().map(|a| self.ast_ty(a, tvs, bind_new)).collect();
                let tc = self.ensure_tuple(args.len());
                Ty::con(&tc, args)
            }
            TypeKind::Qual(ctx, b) => {
                for c in ctx {
                    if let Some(p) = self.constraint(c, tvs, bind_new) {
                        self.qual_preds.push(p);
                    }
                }
                self.ast_ty(b, tvs, bind_new)
            }
            TypeKind::Kinded(b, _) => self.ast_ty(b, tvs, bind_new),
            TypeKind::MulFun(m, a, b) => {
                let m = match m {
                    ast::Multiplicity::One => mult_one(),
                    ast::Multiplicity::Zero => mult_zero(),
                    ast::Multiplicity::Many => mult_many(),
                    ast::Multiplicity::Var(v) => {
                        let vt = ast::Type {
                            kind: TypeKind::Var(v.clone()),
                            span: t.span,
                        };
                        self.ast_ty(&vt, tvs, bind_new)
                    }
                };
                let a = self.ast_ty(a, tvs, bind_new);
                let b = self.ast_ty(b, tvs, bind_new);
                mul_fun(m, a, b)
            }
            TypeKind::Forall(vs, b) => {
                // A nested `forall` (RankNTypes): a polymorphic argument
                // or field.
                let n0 = tvs.len();
                let mut names = Vec::new();
                for (v, k) in vs {
                    let n = self.fresh_name(&format!("{}#", v.name));
                    self.note_tyvar_kind(&n, k.as_ref());
                    tvs.push((v.name.clone(), Ty::Var(n.clone())));
                    names.push(n);
                }
                let saved = std::mem::take(&mut self.qual_preds);
                let body = self.ast_ty(b, tvs, bind_new);
                let preds = std::mem::replace(&mut self.qual_preds, saved);
                if !preds.is_empty() {
                    self.error(err(
                        "E0001",
                        t.span,
                        "a class constraint under a nested `forall` is not supported",
                    ));
                }
                // Variables first seen in the body stay in scope.
                tvs.drain(n0..n0 + names.len());
                if names.is_empty() {
                    body
                } else {
                    Ty::Forall(names, Box::new(body))
                }
            }
        }
    }

    /// A class constraint `C t` of a context.
    pub(crate) fn constraint(
        &mut self,
        c: &ast::Constraint,
        tvs: &mut Vec<(String, Ty)>,
        bind_new: bool,
    ) -> Option<Pred> {
        let class = c.class.text();
        if !self.classes.contains_key(&class) {
            self.error(err(
                "E0201",
                c.class.span,
                format!("class `{}` is not defined", short(&class)),
            ));
            return None;
        }
        let n = self.classes[&class].vars.len();
        if c.types.len() != n {
            self.error(err(
                "E0001",
                c.class.span,
                format!(
                    "the class `{}` takes {n} type argument{}, but the constraint has {}",
                    short(&class),
                    if n == 1 { "" } else { "s" },
                    c.types.len()
                ),
            ));
            return None;
        }
        let tys = c
            .types
            .iter()
            .map(|t| self.ast_ty(t, tvs, bind_new))
            .collect();
        Some(Pred::new(&class, pack_class_tys(tys)))
    }

    /// A signature type: its outermost `forall` binds variables (that
    /// shadow the scoped ones) instead of making a nested polymorphic type.
    /// Returns the explicitly bound variables.
    pub(crate) fn ast_sig_ty(
        &mut self,
        t: &ast::Type,
        tvs: &mut Vec<(String, Ty)>,
    ) -> (Ty, Vec<(String, Ty)>) {
        match &t.kind {
            TypeKind::Forall(vs, b) => {
                let mut bound = Vec::new();
                for (v, k) in vs {
                    let n = self.fresh_name(&format!("{}#", v.name));
                    self.note_tyvar_kind(&n, k.as_ref());
                    let ty = Ty::Var(n);
                    tvs.push((v.name.clone(), ty.clone()));
                    bound.push((v.name.clone(), ty));
                }
                let (ty, more) = self.ast_sig_ty(b, tvs);
                bound.extend(more);
                (ty, bound)
            }
            TypeKind::Qual(ctx, b) => {
                for c in ctx {
                    if let Some(p) = self.constraint(c, tvs, true) {
                        self.qual_preds.push(p);
                    }
                }
                self.ast_sig_ty(b, tvs)
            }
            _ => (self.ast_ty(t, tvs, true), Vec::new()),
        }
    }

    /// The scheme of a type signature.
    pub fn sig_scheme(&mut self, t: &ast::Type) -> Scheme {
        self.sig_scheme_scoped(t).0
    }

    /// The scheme of a type signature and the type variables its explicit
    /// `forall` brings into scope over the definition. Type variables
    /// already in scope are not generalized.
    pub(crate) fn sig_scheme_scoped(&mut self, t: &ast::Type) -> (Scheme, Vec<(String, Ty)>) {
        let mut tvs = self.scoped.clone();
        let n0 = tvs.len();
        let saved = std::mem::take(&mut self.qual_preds);
        let (ty, bound) = self.ast_sig_ty(t, &mut tvs);
        let preds = std::mem::replace(&mut self.qual_preds, saved);
        let s = Scheme {
            vars: tvs
                .drain(n0..)
                .filter_map(|(_, t)| match t {
                    Ty::Var(v) => Some(v),
                    _ => None,
                })
                .collect(),
            preds,
            ty,
        };
        (s, bound)
    }

    // ---- declarations ----

    /// Data types, synonyms and foreign imports of all modules.
    pub fn declare_types(&mut self, modules: &[(String, Vec<Decl>)]) {
        for (m, decls) in modules {
            for d in decls {
                match d {
                    Decl::Data(dd) => {
                        let name = format!("{m}.{}", dd.name.name);
                        let ps: Vec<String> = dd.params.iter().map(|p| p.name.clone()).collect();
                        let n = ps.len();
                        self.add_data(&name, ps, vec![Kind::Star; n], dd.newtype);
                    }
                    Decl::TypeSyn(n, ps, t, _) => {
                        let name = format!("{m}.{}", n.name);
                        self.synonyms.insert(
                            name.clone(),
                            (ps.iter().map(|p| p.name.clone()).collect(), t.clone()),
                        );
                        self.syn_module.insert(name, m.clone());
                    }
                    Decl::Class(c) => self.declare_class_name(m, c),
                    _ => {}
                }
            }
        }
        for (m, decls) in modules {
            for d in decls {
                if let Decl::KindSig(n, k, sp) = d {
                    let name = format!("{m}.{}", n.name);
                    if !self.datas.contains_key(&name) {
                        self.error(err(
                            "E0201",
                            *sp,
                            format!(
                                "the kind signature is for `{}`, which is not a data type of this module",
                                n.name
                            ),
                        ));
                        continue;
                    }
                    let mut arity = 0;
                    let mut r = k;
                    while let ast::KindSig::Arrow(_, b) = r {
                        arity += 1;
                        r = b;
                    }
                    if *r == ast::KindSig::LType {
                        self.ltype_datas.insert(name, arity);
                        self.has_ltype = true;
                    }
                }
            }
        }
        for (m, decls) in modules {
            for d in decls {
                match d {
                    Decl::Data(dd) => self.data_cons(m, dd),
                    Decl::Foreign(f) if f.import => {
                        let s = self.sig_scheme(&f.ty);
                        let name = format!("{m}.{}", f.name.name);
                        self.globals.insert(name.clone(), s.clone());
                        let tvs = s
                            .vars
                            .iter()
                            .map(|v| (v.clone(), kind_of(v, &s.ty)))
                            .collect();
                        self.out.foreigns.push(tast::Foreign {
                            name,
                            callconv: f.callconv.name.clone(),
                            entity: f.entity.clone().unwrap_or_else(|| f.name.name.clone()),
                            tvs,
                            ty: s.ty,
                            span: f.span,
                        });
                    }
                    Decl::Foreign(f) => {
                        self.error(err("E0001", f.span, "`foreign export` is not supported"))
                    }
                    _ => {}
                }
            }
        }
    }

    fn data_cons(&mut self, m: &str, dd: &ast::DataDecl) {
        let name = format!("{m}.{}", dd.name.name);
        let mut tvs: Vec<(String, Ty)> = dd
            .params
            .iter()
            .map(|p| (p.name.clone(), Ty::Var(p.name.clone())))
            .collect();
        let mut tcons = Vec::new();
        if dd.newtype {
            let ok = dd.cons.len() == 1
                && match &dd.cons[0].fields {
                    ConFields::Plain(fs) => fs.len() == 1,
                    ConFields::Record(fs) => fs.len() == 1 && fs[0].0.len() == 1,
                };
            if !ok {
                self.error(err(
                    "E0603",
                    dd.span,
                    "a newtype must have exactly one constructor with exactly one field",
                ));
            }
        }
        for c in &dd.cons {
            let cname = format!("{m}.{}", c.name.name);
            let mut fields = Vec::new();
            let mut strict = Vec::new();
            let mut names = Vec::new();
            match &c.fields {
                ConFields::Plain(fs) => {
                    for (t, s) in fs {
                        fields.push(self.ast_ty(t, &mut tvs, false));
                        strict.push(*s);
                        names.push(None);
                    }
                }
                ConFields::Record(fs) => {
                    for (ns, t, s) in fs {
                        let ty = self.ast_ty(t, &mut tvs, false);
                        for n in ns {
                            fields.push(ty.clone());
                            strict.push(*s);
                            let f = format!("{m}.{}", n.name);
                            self.fields
                                .entry(f.clone())
                                .or_default()
                                .push((cname.clone(), names.len()));
                            names.push(Some(f));
                        }
                    }
                }
            }
            self.datas.get_mut(&name).unwrap().cons.push(cname.clone());
            self.cons.insert(
                cname.clone(),
                ConInfo {
                    data: name.clone(),
                    fields: fields.clone(),
                    strict: strict.clone(),
                    field_names: names,
                },
            );
            tcons.push(tast::ConDecl {
                name: cname,
                fields,
                strict,
            });
        }
        // Kinds of the parameters from their signatures or their uses.
        let kinds: Vec<Kind> = dd
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if let Some(Some(k)) = dd.param_kinds.get(i) {
                    return Kind::from_sig(k);
                }
                let mut k = Kind::Star;
                for c in &tcons {
                    for f in &c.fields {
                        if let Some(n) = applied_arity(&p.name, f) {
                            k = Kind::arity(n);
                        }
                    }
                }
                k
            })
            .collect();
        self.datas.get_mut(&name).unwrap().kinds = kinds.clone();
        self.out.datas.push(tast::DataDecl {
            name,
            params: dd
                .params
                .iter()
                .map(|p| p.name.clone())
                .zip(kinds)
                .collect(),
            cons: tcons,
            newtype: dd.newtype,
        });
    }

    /// Record field selectors as top-level bindings.
    pub fn field_selectors(&mut self) -> Vec<BindGroup> {
        let mut out = Vec::new();
        let mut fields: Vec<(String, Vec<(String, usize)>)> = self
            .fields
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        fields.sort();
        for (f, cons) in fields {
            let data = self.cons[&cons[0].0].data.clone();
            let info = self.datas[&data].clone();
            let pty = Ty::con(
                &data,
                info.params.iter().map(|p| Ty::Var(p.clone())).collect(),
            );
            let fty = self.cons[&cons[0].0].fields[cons[0].1].clone();
            let x = format!("$r{}", out.len());
            let res = format!("$v{}", out.len());
            let inst: Vec<Ty> = info.params.iter().map(|p| Ty::Var(p.clone())).collect();
            let mut clauses = Vec::new();
            for (c, i) in &cons {
                let n = self.cons[c].fields.len();
                let pats = (0..n)
                    .map(|k| {
                        let t = self.cons[c].fields[k].clone();
                        if k == *i {
                            tast::Pat::Var(res.clone(), t)
                        } else {
                            tast::Pat::Wild(t)
                        }
                    })
                    .collect();
                clauses.push(Clause {
                    pats: vec![tast::Pat::Con(c.clone(), inst.clone(), pats)],
                    rhs: Rhs {
                        wheres: Vec::new(),
                        body: vec![(Vec::new(), tast::Expr::Local(res.clone(), fty.clone()))],
                    },
                    span: Span::DUMMY,
                });
            }
            if cons.len() < info.cons.len() {
                clauses.push(Clause {
                    pats: vec![tast::Pat::Wild(pty.clone())],
                    rhs: Rhs {
                        wheres: Vec::new(),
                        body: vec![(
                            Vec::new(),
                            tast::Expr::Error(
                                format!("No match in record selector {}", short(&f)),
                                fty.clone(),
                            ),
                        )],
                    },
                    span: Span::DUMMY,
                });
            }
            let ty = Ty::fun(pty.clone(), fty.clone());
            let body = tast::Expr::Lam(
                x.clone(),
                pty.clone(),
                Box::new(tast::Expr::Match(vec![(x, pty)], clauses, fty)),
            );
            let tvs: Vec<(String, Kind)> = info
                .params
                .iter()
                .cloned()
                .zip(info.kinds.clone())
                .collect();
            self.globals
                .insert(f.clone(), Scheme::poly(info.params.clone(), ty.clone()));
            out.push(BindGroup::NonRec(tast::Bind {
                name: f,
                tvs,
                ty,
                rhs: body,
                span: Span::DUMMY,
                strict: false,
            }));
        }
        out
    }

    // ---- binding groups ----

    /// Infers a group of value declarations. `top` names are globals of
    /// `module`; otherwise the names are unique locals.
    pub fn bind_group(&mut self, decls: &[(String, Decl)], top: bool) -> Vec<BindGroup> {
        // Collect bindings.
        struct B<'a> {
            name: String,
            module: String,
            clauses: Vec<&'a Decl>,
            pat: Option<&'a Decl>,
            span: Span,
        }
        let mut binds: Vec<B> = Vec::new();
        let mut sigs: HashMap<String, (ast::Type, Span)> = HashMap::new();
        for (m, d) in decls {
            let qual = |n: &str| {
                if top {
                    format!("{m}.{n}")
                } else {
                    n.to_string()
                }
            };
            match d {
                Decl::Fun(f, _, _, sp) => {
                    let name = qual(&f.name);
                    match binds.last_mut() {
                        Some(b) if b.name == name && b.pat.is_none() => b.clauses.push(d),
                        _ => binds.push(B {
                            name,
                            module: m.clone(),
                            clauses: vec![d],
                            pat: None,
                            span: *sp,
                        }),
                    }
                }
                Decl::PatBind(_, _, sp) => {
                    let name = self.fresh_name("$p");
                    binds.push(B {
                        name,
                        module: m.clone(),
                        clauses: Vec::new(),
                        pat: Some(d),
                        span: *sp,
                    });
                }
                Decl::TypeSig(ns, t, sp) => {
                    for n in ns {
                        sigs.insert(qual(&n.name), (t.clone(), *sp));
                    }
                }
                _ => {}
            }
        }
        // Signatures are known up front.
        let mut sig_schemes: HashMap<String, Scheme> = HashMap::new();
        let mut sig_scoped: HashMap<String, Vec<(String, Ty)>> = HashMap::new();
        for b in &binds {
            let s = match sigs.get(&b.name) {
                Some((t, _)) => {
                    let (s, bound) = self.sig_scheme_scoped(t);
                    sig_scoped.insert(b.name.clone(), bound);
                    s
                }
                None => match self.extra_sigs.get(&b.name) {
                    Some(s) => s.clone(),
                    None => continue,
                },
            };
            sig_schemes.insert(b.name.clone(), s.clone());
            self.bind_name(&b.name, top, LocalKind::Let(s));
        }
        // Names bound by each binding (pattern bindings bind several).
        let bound: Vec<Vec<String>> = binds
            .iter()
            .map(|b| match b.pat {
                Some(Decl::PatBind(p, _, _)) => {
                    let mut vs = Vec::new();
                    achc_rename_pat_vars(p, &mut vs);
                    vs.into_iter()
                        .map(|v| if top { format!("{}.{v}", b.module) } else { v })
                        .collect()
                }
                _ => vec![b.name.clone()],
            })
            .collect();
        for (n, (_, sp)) in &sigs {
            if !bound.iter().any(|ns| ns.contains(n)) {
                self.error(err(
                    "E0606",
                    *sp,
                    format!(
                        "the type signature for `{}` lacks an accompanying binding",
                        short(n)
                    ),
                ));
            }
        }
        let mut owner: HashMap<String, usize> = HashMap::new();
        for (i, ns) in bound.iter().enumerate() {
            for n in ns {
                owner.insert(n.clone(), i);
            }
        }
        // Dependency graph and SCCs (dependencies first).
        let edges: Vec<Vec<usize>> = binds
            .iter()
            .map(|b| {
                let mut fv = HashSet::new();
                for d in b.clauses.iter().chain(b.pat.iter()) {
                    decl_free_vars(d, &mut fv);
                }
                let mut e: Vec<usize> = fv.iter().filter_map(|v| owner.get(v).copied()).collect();
                e.sort();
                e.dedup();
                e
            })
            .collect();
        let sccs = tarjan(binds.len(), &edges);
        let mut out = Vec::new();
        for scc in sccs {
            let recursive = scc.len() > 1 || edges[scc[0]].contains(&scc[0]);
            // Bindings without signatures are inferred together.
            let infer_ids: Vec<usize> = scc
                .iter()
                .copied()
                .filter(|i| !sig_schemes.contains_key(&binds[*i].name) || binds[*i].pat.is_some())
                .collect();
            let mut results: Vec<tast::Bind> = Vec::new();
            if !infer_ids.is_empty() {
                let wstart = self.wanted.len();
                self.level += 1;
                let mut tys = Vec::new();
                for &i in &infer_ids {
                    let t = self.fresh();
                    for n in &bound[i] {
                        if n == &binds[i].name || binds[i].pat.is_some() {
                            let tv = if binds[i].pat.is_some() {
                                self.fresh()
                            } else {
                                t.clone()
                            };
                            self.bind_name(n, top, LocalKind::Let(Scheme::mono(tv)));
                        }
                    }
                    if binds[i].pat.is_none() {
                        self.rec_tvs.insert(binds[i].name.clone(), Vec::new());
                    }
                    tys.push(t);
                }
                let mut rhss = Vec::new();
                for (k, &i) in infer_ids.iter().enumerate() {
                    let b = &binds[i];
                    match b.pat {
                        Some(Decl::PatBind(p, rhs, sp)) => {
                            let r = self.pat_bind(p, rhs, *sp, &tys[k], top, &b.module);
                            rhss.push(r);
                        }
                        _ => {
                            let e = self.fun_bind(&b.clauses, &tys[k], b.span);
                            rhss.push(vec![(b.name.clone(), tys[k].clone(), e)]);
                        }
                    }
                }
                self.level -= 1;
                // Generalize function bindings; pattern bindings stay
                // monomorphic.
                let generalize = infer_ids.iter().all(|&i| binds[i].pat.is_none());
                // The monomorphism restriction: a group with a binding
                // without arguments does not generalize constrained
                // variables.
                let restricted = !generalize
                    || infer_ids.iter().any(|&i| {
                        matches!(binds[i].clauses.first(), Some(Decl::Fun(_, args, _, _)) if args.is_empty())
                    });
                let mut ms = Vec::new();
                if generalize {
                    for t in &tys {
                        self.gen_metas(t, &mut ms);
                    }
                } else {
                    for r in &rhss {
                        for (_, t, _) in r {
                            self.lower_levels(t);
                        }
                    }
                }
                let new = self.wanted.split_off(wstart);
                let (ctx, mine) = self.generalize_preds(new, &mut ms, &tys, restricted);
                let tvs: Vec<String> = (0..ms.len())
                    .map(|i| self.fresh_name(&format!("{}#", tv_letter(i))))
                    .collect();
                for (m, v) in ms.iter().zip(&tvs) {
                    self.metas[*m as usize].sol = Some(Ty::Var(v.clone()));
                }
                let ctx: Vec<Pred> = ctx
                    .into_iter()
                    .map(|p| Pred::new(&p.class, self.zonk(&p.ty)))
                    .collect();
                let dicts = self.dict_params(&ctx);
                self.solve_with(mine, &dicts);
                for r in rhss {
                    for (name, t, e) in r {
                        let t = self.zonk(&t);
                        let s = Scheme {
                            vars: tvs.clone(),
                            preds: ctx.clone(),
                            ty: t.clone(),
                        };
                        self.bind_name(&name, top, LocalKind::Let(s));
                        self.rec_tvs.insert(name.clone(), tvs.clone());
                        self.rec_dicts.insert(name.clone(), dicts.clone());
                        let (e, full) = wrap_dicts(&dicts, e, t);
                        let ktvs = tvs
                            .iter()
                            .map(|v| (v.clone(), self.kind_in(v, &full)))
                            .collect();
                        let strict = !recursive && self.strict_locals.contains(&name);
                        results.push(tast::Bind {
                            name,
                            tvs: ktvs,
                            ty: full,
                            rhs: e,
                            span: Span::DUMMY,
                            strict,
                        });
                    }
                }
            }
            // Bindings with signatures.
            for &i in &scc {
                let b = &binds[i];
                let Some(s) = sig_schemes.get(&b.name).cloned() else {
                    continue;
                };
                if b.pat.is_some() {
                    continue;
                }
                self.rec_tvs.insert(b.name.clone(), s.vars.clone());
                self.rec_dicts.insert(b.name.clone(), Vec::new());
                let dicts = self.dict_params(&s.preds);
                let gstart = self.givens.len();
                for (p, (d, t)) in s.preds.iter().zip(&dicts) {
                    self.givens
                        .push((p.clone(), tast::Expr::Local(d.clone(), t.clone())));
                }
                let wstart = self.wanted.len();
                let scoped_len = self.scoped.len();
                if let Some(bound) = sig_scoped.get(&b.name) {
                    self.scoped.extend(bound.iter().cloned());
                }
                self.level += 1;
                let e = self.fun_bind(&b.clauses, &s.ty, b.span);
                self.level -= 1;
                self.scoped.truncate(scoped_len);
                let new = self.wanted.split_off(wstart);
                self.solve_sig(new, &s.vars);
                self.givens.truncate(gstart);
                let (e, full) = wrap_dicts(&dicts, e, s.ty.clone());
                let tvs = s
                    .vars
                    .iter()
                    .map(|v| (v.clone(), self.kind_in(v, &full)))
                    .collect();
                results.push(tast::Bind {
                    name: b.name.clone(),
                    tvs,
                    ty: full,
                    rhs: e,
                    span: b.span,
                    strict: false,
                });
            }
            if recursive {
                out.push(BindGroup::Rec(results));
            } else {
                // A non-recursive pattern binding: the shared right-hand
                // side first, then the selectors.
                out.extend(results.into_iter().map(BindGroup::NonRec));
            }
        }
        // Pattern-bound variables with a signature: check the annotation.
        for (n, (t, sp)) in &sigs {
            if binds.iter().any(|b| &b.name == n) {
                continue;
            }
            if let Some(LocalKind::Let(s)) = self.lookup_kind(n, top) {
                let want = self.sig_scheme(t);
                if !want.vars.is_empty() {
                    continue;
                }
                let got = s.ty.clone();
                self.unify(*sp, &want.ty, &got);
            }
        }
        out
    }

    fn lookup_kind(&self, n: &str, top: bool) -> Option<LocalKind> {
        if top {
            self.globals.get(n).cloned().map(LocalKind::Let)
        } else {
            self.locals.get(n).cloned()
        }
    }

    fn bind_name(&mut self, n: &str, top: bool, k: LocalKind) {
        if top {
            if let LocalKind::Let(s) = k {
                self.globals.insert(n.to_string(), s);
            }
        } else {
            self.locals.insert(n.to_string(), k);
        }
    }

    /// `p = rhs`: binds a fresh variable to the right-hand side and each
    /// pattern variable to a match on it (lazily, as Haskell requires).
    fn pat_bind(
        &mut self,
        p: &ast::Pat,
        rhs: &ast::Rhs,
        sp: Span,
        t: &Ty,
        top: bool,
        module: &str,
    ) -> Vec<(String, Ty, tast::Expr)> {
        let whole = self.fresh_name("$pb");
        let whole = if top {
            format!("{module}.{whole}")
        } else {
            whole
        };
        let e = self.rhs_expr(rhs, t, sp);
        let tp = self.pat(p, t);
        if !top && matches!(strip_paren(p).kind, PatKind::Bang(_)) {
            self.strict_locals.insert(whole.clone());
        }
        let mut out = vec![(whole.clone(), t.clone(), e)];
        let mut vars = Vec::new();
        tpat_vars(&tp, &mut vars);
        for (v, vt) in vars {
            let gname = if top {
                format!("{module}.{v}")
            } else {
                v.clone()
            };
            // The pattern variable `v` itself is bound to the matched value.
            let body = tast::Expr::Match(
                vec![(whole.clone() + "$", t.clone())],
                vec![Clause {
                    pats: vec![tp.clone()],
                    rhs: Rhs {
                        wheres: Vec::new(),
                        body: vec![(Vec::new(), tast::Expr::Local(v.clone(), vt.clone()))],
                    },
                    span: sp,
                }],
                vt.clone(),
            );
            // `whole$` is bound to the shared right-hand side.
            let e = tast::Expr::Let(
                vec![BindGroup::NonRec(tast::Bind {
                    name: whole.clone() + "$",
                    tvs: Vec::new(),
                    ty: t.clone(),
                    rhs: tast::Expr::Global(whole.clone(), vec![Ty::Con(REC.into())]),
                    span: sp,
                    strict: false,
                })],
                Box::new(body),
            );
            if let Some(LocalKind::Let(s)) = self.lookup_kind(&gname, top) {
                self.unify(sp, &s.ty, &vt);
            }
            out.push((gname, vt, e));
        }
        out
    }

    /// The clauses of a function as nested lambdas over a match.
    fn fun_bind(&mut self, clauses: &[&Decl], ty: &Ty, span: Span) -> tast::Expr {
        let arity = match clauses[0] {
            Decl::Fun(_, args, _, _) => args.len(),
            _ => 0,
        };
        for c in &clauses[1..] {
            if let Decl::Fun(f, args, _, sp) = c
                && args.len() != arity
            {
                self.error(
                    err(
                        "E0605",
                        *sp,
                        format!(
                            "equations for `{}` have different numbers of arguments",
                            short(&f.name)
                        ),
                    )
                    .with_secondary(span, "the first equation is here"),
                );
                return tast::Expr::Error("arity".into(), ty.clone());
            }
        }
        // Split the expected type into argument and result types.
        let mut args = Vec::new();
        let mut res = ty.clone();
        let mut mults = Vec::new();
        if self.has_ltype
            && let Decl::Fun(f, ..) = clauses[0]
        {
            self.pat_tys.insert(f.span, ty.clone());
        }
        // U-Arg (03-type-system.md §2.2): an arrow of a binding without a
        // signature becomes linear when its argument is linear-only.
        let mut open_mults = Vec::new();
        for i in 0..arity {
            let r = self.shallow(&res);
            if self.has_ltype && matches!(r, Ty::Meta(_)) {
                let (m, a, b) = (self.fresh(), self.fresh(), self.fresh());
                self.unify(span, &r, &mul_fun(m.clone(), a.clone(), b.clone()));
                open_mults.push((i, m.clone()));
                mults.push(Some(m));
                args.push(a);
                res = b;
                continue;
            }
            if let Some((m, a, b)) = as_mul_fun(&r) {
                mults.push(Some(m.clone()));
                args.push(a.clone());
                res = b.clone();
                continue;
            }
            mults.push(None);
            match r {
                Ty::Fun(a, b) => {
                    args.push(*a);
                    res = *b;
                }
                _ => {
                    let a = self.fresh();
                    let b = self.fresh();
                    self.unify(span, &r, &Ty::fun(a.clone(), b.clone()));
                    args.push(a);
                    res = b;
                }
            }
        }
        // `x = e` without guards or `where` is just the expression.
        if arity == 0
            && clauses.len() == 1
            && let Decl::Fun(_, _, rhs, sp) = clauses[0]
            && rhs.wheres.is_empty()
            && let ast::RhsBody::Plain(e) = &rhs.body
        {
            let _ = sp;
            return self.expr(e, &res);
        }
        let vars: Vec<(String, Ty)> = args
            .iter()
            .map(|t| (self.fresh_name("$a"), t.clone()))
            .collect();
        let mut tclauses = Vec::new();
        for c in clauses {
            if let Decl::Fun(_, ps, rhs, sp) = c {
                for (p, m) in ps.iter().zip(&mults) {
                    if let Some(m) = m {
                        self.lin_pats.insert(p.span, m.clone());
                    }
                }
                let pats = ps.iter().zip(&args).map(|(p, t)| self.pat(p, t)).collect();
                let rhs = self.rhs(rhs, &res);
                tclauses.push(Clause {
                    pats,
                    rhs,
                    span: *sp,
                });
            }
        }
        for (i, m) in open_mults {
            if matches!(self.shallow(&m), Ty::Meta(_)) {
                let lin = self.is_ltype(&args[i]);
                self.unify(span, &m, &if lin { mult_one() } else { mult_many() });
            }
        }
        let mut e = tast::Expr::Match(vars.clone(), tclauses, res);
        for (v, t) in vars.into_iter().rev() {
            e = tast::Expr::Lam(v, t, Box::new(e));
        }
        e
    }

    fn rhs(&mut self, r: &ast::Rhs, res: &Ty) -> Rhs {
        let wheres = self.local_decls(&r.wheres);
        let body = match &r.body {
            ast::RhsBody::Plain(e) => vec![(Vec::new(), self.marked(e, res))],
            ast::RhsBody::Guarded(gs) => gs
                .iter()
                .map(|g| {
                    let guards = g.guards.iter().map(|s| self.guard(s)).collect();
                    (guards, self.marked(&g.body, res))
                })
                .collect(),
        };
        Rhs { wheres, body }
    }

    /// A right-hand side as an expression (pattern bindings).
    fn rhs_expr(&mut self, r: &ast::Rhs, t: &Ty, sp: Span) -> tast::Expr {
        if r.wheres.is_empty()
            && let ast::RhsBody::Plain(e) = &r.body
        {
            return self.expr(e, t);
        }
        let rhs = self.rhs(r, t);
        tast::Expr::Match(
            Vec::new(),
            vec![Clause {
                pats: Vec::new(),
                rhs,
                span: sp,
            }],
            t.clone(),
        )
    }

    fn guard(&mut self, s: &Stmt) -> Guard {
        match s {
            Stmt::Expr(e) => {
                let b = self.bool_ty();
                Guard::Bool(self.expr(e, &b))
            }
            Stmt::Bind(p, e, _) => {
                let t = self.fresh();
                let te = self.expr(e, &t);
                let tp = self.pat(p, &t);
                Guard::Bind(tp, te)
            }
            Stmt::Let(ds, _) => Guard::Let(self.local_decls(ds)),
        }
    }

    fn local_decls(&mut self, ds: &[Decl]) -> Vec<BindGroup> {
        let v: Vec<(String, Decl)> = ds.iter().map(|d| (String::new(), d.clone())).collect();
        self.bind_group(&v, false)
    }

    // ---- built-in names ----

    fn bool_ty(&self) -> Ty {
        Ty::Con(prim("Bool"))
    }

    fn lit_ty(&mut self, name: &str) -> Ty {
        Ty::Con(prim(name))
    }

    /// A reference to a global needed by syntactic sugar.
    /// A name wired into syntax (`Prelude.x`): the base library defines it
    /// in `Cirka.Base` (re-exported by `Prelude`); small test preludes
    /// define it in `Prelude` itself.
    /// The type of integer literals given to `fromInteger`: the base
    /// library's `Integer`, or `Int` with a small test prelude.
    pub(crate) fn integer_ty(&self) -> Ty {
        let b = format!("{}.Integer", achc_rename::BASE_MODULE);
        if self.datas.contains_key(&b) {
            Ty::Con(b)
        } else {
            Ty::Con(prim("Int"))
        }
    }

    /// An `Integer` literal (`cirkaIntegerLit n`, or `cirkaIntegerLitBig
    /// "digits"` beyond `Int`); a plain `Int` literal with a small prelude.
    pub(crate) fn integer_lit(&self, n: i128) -> tast::Expr {
        let small = self.wired("Prelude.cirkaIntegerLit");
        if !self.globals.contains_key(&small) {
            return tast::Expr::Lit(tast::Lit::Int(n as i64));
        }
        match i64::try_from(n) {
            Ok(k) => tast::Expr::App(
                Box::new(tast::Expr::Global(small, Vec::new())),
                Box::new(tast::Expr::Lit(tast::Lit::Int(k))),
            ),
            Err(_) => tast::Expr::App(
                Box::new(tast::Expr::Global(
                    self.wired("Prelude.cirkaIntegerLitBig"),
                    Vec::new(),
                )),
                Box::new(tast::Expr::Lit(tast::Lit::Str(n.to_string()))),
            ),
        }
    }

    /// [`Self::integer_lit`] of a decimal string (with optional `-`).
    pub(crate) fn integer_lit_str(&self, s: &str) -> tast::Expr {
        match s.parse::<i128>() {
            Ok(n)
                if i64::try_from(n).is_ok()
                    || !self
                        .globals
                        .contains_key(&self.wired("Prelude.cirkaIntegerLit")) =>
            {
                self.integer_lit(n)
            }
            _ => tast::Expr::App(
                Box::new(tast::Expr::Global(
                    self.wired("Prelude.cirkaIntegerLitBig"),
                    Vec::new(),
                )),
                Box::new(tast::Expr::Lit(tast::Lit::Str(s.to_string()))),
            ),
        }
    }

    pub(crate) fn wired(&self, name: &str) -> String {
        if let Some(short) = name.strip_prefix("Prelude.") {
            let b = format!("{}.{short}", achc_rename::BASE_MODULE);
            if self.globals.contains_key(&b)
                || self.classes.contains_key(&b)
                || self.cons.contains_key(&b)
            {
                return b;
            }
        }
        name.to_string()
    }

    /// Whether matching `p` can fail (it needs `MonadFail` in `do`).
    fn failable(&self, p: &tast::Pat) -> bool {
        match p {
            tast::Pat::Var(..) | tast::Pat::Wild(_) | tast::Pat::Lazy(_) => false,
            tast::Pat::As(_, _, q) | tast::Pat::Bang(q) => self.failable(q),
            tast::Pat::Con(c, _, ps) => {
                let many = self
                    .cons
                    .get(c)
                    .and_then(|k| self.datas.get(&k.data))
                    .is_none_or(|d| d.cons.len() != 1);
                many || ps.iter().any(|q| self.failable(q))
            }
            tast::Pat::Lit(..) | tast::Pat::OverLit(..) => true,
        }
    }

    fn sugar(&mut self, name: &str, span: Span) -> (tast::Expr, Ty) {
        let name = &self.wired(name);
        match self.globals.get(name).cloned() {
            Some(s) => self.global_ref(name, &s, span),
            None => {
                self.error(err(
                    "E0201",
                    span,
                    format!("this syntax needs `{name}`, which is not defined"),
                ));
                let t = self.fresh();
                (tast::Expr::Error(name.into(), t.clone()), t)
            }
        }
    }

    fn app(
        &mut self,
        span: Span,
        f: (tast::Expr, Ty),
        a: tast::Expr,
        aty: &Ty,
    ) -> (tast::Expr, Ty) {
        let r = self.fresh();
        self.unify(span, &f.1, &Ty::fun(aty.clone(), r.clone()));
        (tast::Expr::App(Box::new(f.0), Box::new(a)), r)
    }

    fn con_expr(&mut self, name: &str) -> (tast::Expr, Ty) {
        let ci = self.cons[name].clone();
        let di = self.datas[&ci.data].clone();
        let inst: Vec<Ty> = di.params.iter().map(|_| self.fresh()).collect();
        let sub: Vec<(String, Ty)> = di
            .params
            .iter()
            .cloned()
            .zip(inst.iter().cloned())
            .collect();
        let res = Ty::con(&ci.data, inst.clone());
        let t = Ty::funs(ci.fields.iter().map(|f| f.subst(&sub)).collect(), res);
        (tast::Expr::Con(name.to_string(), inst), t)
    }

    // ---- expressions ----

    pub fn expr(&mut self, e: &ast::Expr, expected: &Ty) -> tast::Expr {
        if let Ty::Forall(vs, body) = self.shallow(expected) {
            return self.check_poly(e, &vs, &body);
        }
        let before = self.errors.len();
        let (te, t) = self.infer(e, Some(expected));
        if self.errors.len() == before {
            self.unify(e.span, expected, &t);
        } else {
            // Do not report the same mistake again for the enclosing
            // expression.
            let _ = self.unify_raw(expected, &t);
        }
        te
    }

    /// Checks `e` against `forall vs. body` (RankNTypes): with the
    /// variables rigid, as a local polymorphic binding.
    fn check_poly(&mut self, e: &ast::Expr, vs: &[String], body: &Ty) -> tast::Expr {
        let sk: Vec<String> = vs
            .iter()
            .map(|v| {
                self.fresh_name(&format!(
                    "{}#",
                    short(v).trim_end_matches(|c: char| c == '#' || c.is_ascii_digit())
                ))
            })
            .collect();
        let sub: Vec<(String, Ty)> = vs
            .iter()
            .cloned()
            .zip(sk.iter().map(|v| Ty::Var(v.clone())))
            .collect();
        let inner = body.subst(&sub);
        let wstart = self.wanted.len();
        self.level += 1;
        let te = self.expr(e, &inner);
        self.level -= 1;
        let new = self.wanted.split_off(wstart);
        self.solve_sig(new, &sk);
        let name = self.fresh_name("$poly");
        let tvs = sk
            .iter()
            .map(|v| (v.clone(), self.kind_in(v, &inner)))
            .collect();
        tast::Expr::Let(
            vec![BindGroup::NonRec(tast::Bind {
                name: name.clone(),
                tvs,
                ty: inner.clone(),
                rhs: te,
                span: e.span,
                strict: false,
            })],
            Box::new(tast::Expr::Local(name, Ty::Forall(sk, Box::new(inner)))),
        )
    }

    fn infer(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> (tast::Expr, Ty) {
        let sp = e.span;
        match &e.kind {
            ExprKind::Var(q) => self.var(q),
            ExprKind::Con(q) => {
                let name = q.text();
                if let Some(n) = tuple_arity(&name) {
                    self.ensure_tuple(n);
                }
                if !self.cons.contains_key(&name) {
                    self.error(err(
                        "E0201",
                        sp,
                        format!("constructor `{}` is not defined", short(&name)),
                    ));
                    let t = self.fresh();
                    return (tast::Expr::Error(name, t.clone()), t);
                }
                self.con_expr(&name)
            }
            ExprKind::Lit(l) => self.lit(l, sp),
            ExprKind::App(f, a) => {
                let (tf, ft) = self.infer(f, None);
                let ft = self.shallow(&ft);
                if let Some((m, at, rt)) = as_mul_fun(&ft) {
                    self.lin_args.insert(a.span, m.clone());
                    let (at, rt) = (at.clone(), rt.clone());
                    let at = self.con_arg_ty(a, at);
                    if self.has_ltype {
                        self.arg_tys.insert(a.span, at.clone());
                    }
                    let ta = self.marked(a, &at);
                    return (tast::Expr::App(Box::new(tf), Box::new(ta)), rt);
                }
                let (at, rt) = match ft {
                    Ty::Fun(a, r) => (*a, *r),
                    other => {
                        let a = self.fresh();
                        let r = self.fresh();
                        if !self.unify(f.span, &Ty::fun(a.clone(), r.clone()), &other) {
                            return (tast::Expr::Error("app".into(), r.clone()), r);
                        }
                        (a, r)
                    }
                };
                let at = self.con_arg_ty(a, at);
                if self.has_ltype {
                    self.arg_tys.insert(a.span, at.clone());
                }
                if self.has_ltype && !con_headed(f) {
                    let from_inst =
                        matches!(self.shallow(&at), Ty::Meta(id) if self.inst_metas.contains(&id));
                    if !from_inst {
                        self.omega_args.push((a.span, at.clone()));
                    }
                }
                let ta = self.marked(a, &at);
                (tast::Expr::App(Box::new(tf), Box::new(ta)), rt)
            }
            ExprKind::Lam(ps, body) => {
                // Parameter types and multiplicities from the expected type.
                let mut tys: Vec<Ty> = Vec::new();
                let mut mults: Vec<Ty> = Vec::new();
                let mut cur = expected.map(|t| self.shallow(t));
                for p in ps {
                    match cur.as_ref().map(|t| (as_mul_fun(t), t)) {
                        Some((Some((m, a, b)), _)) => {
                            let (m, a, b) = (self.shallow(m), a.clone(), self.shallow(b));
                            if m != mult_many() {
                                self.lin_pats.insert(p.span, m.clone());
                            }
                            mults.push(m);
                            tys.push(a);
                            cur = Some(b);
                        }
                        Some((None, Ty::Fun(a, b))) => {
                            let (a, b) = ((**a).clone(), self.shallow(b));
                            mults.push(mult_many());
                            tys.push(a);
                            cur = Some(b);
                        }
                        _ => {
                            mults.push(mult_many());
                            tys.push(self.fresh());
                            cur = None;
                        }
                    }
                }
                let res = match cur {
                    Some(t) if tys.len() == ps.len() => t,
                    _ => self.fresh(),
                };
                let simple: Option<Vec<String>> = ps
                    .iter()
                    .map(|p| match &p.kind {
                        PatKind::Var(v) => Some(v.name.clone()),
                        _ => None,
                    })
                    .collect();
                let te = if let Some(names) = simple {
                    for (n, t) in names.iter().zip(&tys) {
                        self.locals.insert(n.clone(), LocalKind::Mono(t.clone()));
                    }
                    let b = self.marked(body, &res);
                    let mut e = b;
                    for (n, t) in names.into_iter().zip(&tys).rev() {
                        e = tast::Expr::Lam(n, t.clone(), Box::new(e));
                    }
                    e
                } else {
                    let pats: Vec<tast::Pat> =
                        ps.iter().zip(&tys).map(|(p, t)| self.pat(p, t)).collect();
                    let b = self.marked(body, &res);
                    let vars: Vec<(String, Ty)> = tys
                        .iter()
                        .map(|t| (self.fresh_name("$l"), t.clone()))
                        .collect();
                    let m = tast::Expr::Match(
                        vars.clone(),
                        vec![Clause {
                            pats,
                            rhs: Rhs {
                                wheres: Vec::new(),
                                body: vec![(Vec::new(), b)],
                            },
                            span: sp,
                        }],
                        res.clone(),
                    );
                    let mut e = m;
                    for (n, t) in vars.into_iter().rev() {
                        e = tast::Expr::Lam(n, t, Box::new(e));
                    }
                    e
                };
                let ty = tys
                    .into_iter()
                    .zip(mults)
                    .rev()
                    .fold(res, |r, (a, m)| mul_fun(m, a, r));
                (te, ty)
            }
            ExprKind::Let(ds, body) => {
                let gs = self.local_decls(ds);
                let (b, t) = self.infer(body, expected);
                let b = self.mark(body.span, b, &t);
                (tast::Expr::Let(gs, Box::new(b)), t)
            }
            ExprKind::If(c, t, f) => {
                let b = self.bool_ty();
                let tc = self.expr(c, &b);
                let res = expected.cloned().unwrap_or_else(|| self.fresh());
                let tt = self.marked(t, &res);
                let tf = self.marked(f, &res);
                let e = self.match_on(
                    tc,
                    b,
                    vec![(self.con_pat0("True"), tt), (self.con_pat0("False"), tf)],
                    res.clone(),
                    sp,
                );
                (e, res)
            }
            ExprKind::MultiIf(gs) => {
                let res = expected.cloned().unwrap_or_else(|| self.fresh());
                let body = gs
                    .iter()
                    .map(|g| {
                        let guards = g.guards.iter().map(|s| self.guard(s)).collect();
                        (guards, self.marked(&g.body, &res))
                    })
                    .collect();
                let m = tast::Expr::Match(
                    Vec::new(),
                    vec![Clause {
                        pats: Vec::new(),
                        rhs: Rhs {
                            wheres: Vec::new(),
                            body,
                        },
                        span: sp,
                    }],
                    res.clone(),
                );
                (m, res)
            }
            ExprKind::Case(s, alts) => {
                let st = self.fresh();
                let ts = self.expr(s, &st);
                let res = expected.cloned().unwrap_or_else(|| self.fresh());
                let clauses = self.alts(alts, &st, &res);
                let v = self.fresh_name("$c");
                let m = tast::Expr::Match(vec![(v.clone(), st.clone())], clauses, res.clone());
                (self.let_mono(&v, st, ts, m), res)
            }
            ExprKind::LamCase(alts) => {
                let st = self.fresh();
                let mut mult = mult_many();
                let res = expected
                    .and_then(|t| {
                        let t = self.shallow(t);
                        if let Some((m, _, r)) = as_mul_fun(&t) {
                            mult = m.clone();
                            return Some(r.clone());
                        }
                        match t {
                            Ty::Fun(_, r) => Some(*r),
                            _ => None,
                        }
                    })
                    .unwrap_or_else(|| self.fresh());
                if mult != mult_many() {
                    self.lin_pats.insert(sp, mult.clone());
                }
                let clauses = self.alts(alts, &st, &res);
                let v = self.fresh_name("$c");
                let m = tast::Expr::Match(vec![(v.clone(), st.clone())], clauses, res.clone());
                (
                    tast::Expr::Lam(v, st.clone(), Box::new(m)),
                    mul_fun(mult, st, res),
                )
            }
            ExprKind::Do(ss) => {
                let (e, t) = self.do_stmts(ss, sp);
                if self.has_ltype {
                    self.do_tys.insert(sp, t.clone());
                }
                (e, t)
            }
            ExprKind::Tuple(es) => {
                let tc = self.ensure_tuple(es.len());
                let (mut f, mut ft) = self.con_expr(&tc);
                for x in es {
                    let a = self.fresh();
                    let ta = self.expr(x, &a);
                    (f, ft) = self.app(x.span, (f, ft), ta, &a);
                }
                (f, ft)
            }
            ExprKind::TupleSection(parts) => {
                let tc = self.ensure_tuple(parts.len());
                let (mut f, mut ft) = self.con_expr(&tc);
                let mut lams = Vec::new();
                for p in parts {
                    let a = self.fresh();
                    let ta = match p {
                        Some(x) => self.expr(x, &a),
                        None => {
                            let v = self.fresh_name("$s");
                            lams.push((v.clone(), a.clone()));
                            tast::Expr::Local(v, a.clone())
                        }
                    };
                    (f, ft) = self.app(sp, (f, ft), ta, &a);
                }
                for (v, t) in lams.into_iter().rev() {
                    ft = Ty::fun(t.clone(), ft);
                    f = tast::Expr::Lam(v, t, Box::new(f));
                }
                (f, ft)
            }
            ExprKind::List(es) => {
                let a = self.fresh();
                let items: Vec<tast::Expr> = es.iter().map(|x| self.expr(x, &a)).collect();
                (self.list_lit(items, &a), Ty::list(a))
            }
            ExprKind::Range(from, then, to) => {
                let name = match (then.is_some(), to.is_some()) {
                    (false, false) => "Prelude.enumFrom",
                    (true, false) => "Prelude.enumFromThen",
                    (false, true) => "Prelude.enumFromTo",
                    (true, true) => "Prelude.enumFromThenTo",
                };
                let mut f = self.sugar(name, sp);
                for x in std::iter::once(from).chain(then.iter()).chain(to.iter()) {
                    let a = self.fresh();
                    let ta = self.expr(x, &a);
                    f = self.app(x.span, f, ta, &a);
                }
                f
            }
            ExprKind::ListComp(h, qs) => {
                let a = self.fresh();
                let e = self.list_comp(h, qs, &a, sp);
                (e, Ty::list(a))
            }
            ExprKind::LeftSection(x, op) => {
                // `(x op)` is `(op x)`.
                let f = self.var_or_con(op);
                let a = self.fresh();
                let tx = self.expr(x, &a);
                self.app(sp, f, tx, &a)
            }
            ExprKind::RightSection(op, x) => {
                // `(op x)` is `\y -> op y x`.
                let (f, ft) = self.var_or_con(op);
                let a = self.fresh();
                let b = self.fresh();
                let r = self.fresh();
                self.unify(
                    op.span,
                    &Ty::funs(vec![a.clone(), b.clone()], r.clone()),
                    &ft,
                );
                let tx = self.expr(x, &b);
                let y = self.fresh_name("$s");
                let body = tast::Expr::App(
                    Box::new(tast::Expr::App(
                        Box::new(f),
                        Box::new(tast::Expr::Local(y.clone(), a.clone())),
                    )),
                    Box::new(tx),
                );
                (tast::Expr::Lam(y, a.clone(), Box::new(body)), Ty::fun(a, r))
            }
            ExprKind::Typed(x, t) => self.typed(x, t, sp),
            ExprKind::RecordCon(c, fs) => self.record_con(c, fs, sp),
            ExprKind::RecordUpd(x, fs) => self.record_upd(x, fs, sp),
            ExprKind::OpSeq(_) | ExprKind::Paren(_) => {
                self.error(err("E0102", sp, "internal: unresolved operator expression"));
                let t = self.fresh();
                (tast::Expr::Error("ops".into(), t.clone()), t)
            }
            ExprKind::Error => {
                let t = self.fresh();
                (tast::Expr::Error("error".into(), t.clone()), t)
            }
        }
    }

    /// LinearTypes: data constructors are linear functions. An argument
    /// headed by a constructor is checked against the expected type with
    /// its linear arrows made unrestricted (the multiplicity is erased and
    /// the linearity checker treats constructor fields as linear).
    fn con_arg_ty(&self, a: &ast::Expr, t: Ty) -> Ty {
        if !con_headed(a) {
            return t;
        }
        let t = self.shallow(&t);
        match as_mul_fun(&t) {
            Some((_, x, r)) => {
                let r = self.con_arg_ty(a, r.clone());
                Ty::fun(x.clone(), r)
            }
            None => t,
        }
    }

    fn var_or_con(&mut self, q: &ast::QName) -> (tast::Expr, Ty) {
        if q.is_con() {
            let n = q.text();
            if self.cons.contains_key(&n) {
                return self.con_expr(&n);
            }
        }
        self.var(q)
    }

    /// `error` of the base library becomes `cirkaErrorCalledAt "<site>"`,
    /// so that the message has GHC's `CallStack` line.
    fn error_with_call_site(&mut self, name: &str, sp: Span) -> Option<(tast::Expr, Ty)> {
        let base = achc_rename::BASE_MODULE;
        let (fun, helper) = [
            ("error", "cirkaErrorCalledAt"),
            ("undefined", "cirkaUndefinedAt"),
        ]
        .into_iter()
        .find(|(f, _)| name == format!("{base}.{f}"))?;
        let helper = format!("{base}.{helper}");
        let s = self.globals.get(&helper)?.clone();
        let (sm, mods) = self.sources.as_ref()?;
        let file = sm.get(sp.file)?;
        let (line, col) = file.line_col(sp.start);
        let module = mods.get(&sp.file.0)?;
        let site = format!(
            "{fun}, called at {}:{line}:{col} in main:{module}",
            file.name
        );
        let f = self.let_ref(&helper, &s, sp);
        let st = Ty::list(Ty::Con(prim("Char")));
        Some(self.app(sp, f, tast::Expr::Lit(tast::Lit::Str(site)), &st))
    }

    fn var(&mut self, q: &ast::QName) -> (tast::Expr, Ty) {
        let name = q.text();
        if q.qual.is_none() {
            match self.locals.get(&name).cloned() {
                Some(LocalKind::Mono(t)) => {
                    // A polymorphic argument (RankNTypes) is instantiated
                    // at each use.
                    if let Ty::Forall(vs, b) = self.shallow(&t) {
                        let ms: Vec<Ty> = vs.iter().map(|_| self.fresh()).collect();
                        let sub: Vec<(String, Ty)> =
                            vs.iter().cloned().zip(ms.iter().cloned()).collect();
                        return (tast::Expr::Global(name, ms), b.subst(&sub));
                    }
                    return (tast::Expr::Local(name, t.clone()), t);
                }
                Some(LocalKind::Let(s)) => return self.let_ref(&name, &s, q.span),
                None => {
                    self.error(err(
                        "E0201",
                        q.span,
                        format!("`{}` is not defined", short(&name)),
                    ));
                    let t = self.fresh();
                    return (tast::Expr::Error(name, t.clone()), t);
                }
            }
        }
        if q.is_con() {
            if self.cons.contains_key(&name) {
                return self.con_expr(&name);
            }
            self.error(err(
                "E0201",
                q.span,
                format!("constructor `{}` is not defined", short(&name)),
            ));
            let t = self.fresh();
            return (tast::Expr::Error(name, t.clone()), t);
        }
        if let Some(e) = self.error_with_call_site(&name, q.span) {
            return e;
        }
        match self.globals.get(&name).cloned() {
            Some(s) => self.let_ref(&name, &s, q.span),
            None => {
                self.error(err(
                    "E0201",
                    q.span,
                    format!("`{}` is not defined", short(&name)),
                ));
                let t = self.fresh();
                (tast::Expr::Error(name, t.clone()), t)
            }
        }
    }

    /// A reference to a let-bound name (monomorphic inside its own group).
    fn let_ref(&mut self, name: &str, s: &Scheme, span: Span) -> (tast::Expr, Ty) {
        if s.vars.is_empty()
            && let Some(tvs) = self.rec_tvs.get(name)
            && tvs.is_empty()
        {
            return (
                tast::Expr::Global(name.to_string(), vec![Ty::Con(REC.into())]),
                s.ty.clone(),
            );
        }
        let (e, t) = self.global_ref(name, s, span);
        if self.has_ltype
            && let tast::Expr::Global(_, inst) = head_global(&e)
        {
            for (v, m) in s.vars.iter().zip(inst.clone()) {
                if !self.ltype_vars.contains(v) {
                    if let Ty::Meta(id) = m {
                        self.inst_metas.insert(id);
                    }
                    self.inst_checks
                        .push((span, name.to_string(), v.clone(), m));
                }
            }
        }
        (e, t)
    }

    /// Checks an expression; with linear-only types, marks it with its span
    /// for the insertions of FEAT-0122.
    fn marked(&mut self, e: &ast::Expr, t: &Ty) -> tast::Expr {
        let te = self.expr(e, t);
        self.mark(e.span, te, t)
    }

    fn mark(&self, sp: Span, e: tast::Expr, t: &Ty) -> tast::Expr {
        if self.has_ltype {
            tast::Expr::Mark(sp, Box::new(e), t.clone())
        } else {
            e
        }
    }

    /// Records the kind annotation of a type variable (`LType`).
    fn note_tyvar_kind(&mut self, v: &str, k: Option<&ast::KindSig>) {
        if k == Some(&ast::KindSig::LType) {
            self.ltype_vars.insert(v.to_string());
            self.has_ltype = true;
        }
    }

    /// Whether values of a type are linear-only (kind `LType`): `Unique`
    /// and types declared `:: LType`, data types with such a field (the lub
    /// of the field kinds, 03-type-system.md §1.1) and `LType` variables.
    pub fn is_ltype(&self, t: &Ty) -> bool {
        if !self.has_ltype {
            return false;
        }
        self.is_ltype_in(&self.zonk(t), &mut Vec::new())
    }

    /// Whether a value of a type may hold a closure (and so use the
    /// variables it captures more than once): functions, `IO` actions,
    /// type variables and data types with such a field.
    pub fn may_hold_closure(&self, t: &Ty) -> bool {
        self.closure_in(&self.zonk(t), &mut Vec::new())
    }

    fn closure_in(&self, t: &Ty, visiting: &mut Vec<String>) -> bool {
        match t {
            Ty::Fun(..) | Ty::Forall(..) | Ty::Var(_) | Ty::Meta(_) => true,
            Ty::Con(_) | Ty::App(..) => {
                let (h, args) = t.spine();
                let Ty::Con(n) = h else {
                    return true;
                };
                if *n == fun_tc() || *n == prim("IO") {
                    return true;
                }
                if visiting.contains(n) {
                    return false;
                }
                let Some(d) = self.datas.get(n) else {
                    // Primitive types.
                    return args.iter().any(|a| self.closure_in(a, visiting));
                };
                if args.len() != d.params.len() {
                    return true;
                }
                let sub: Vec<(String, Ty)> = d
                    .params
                    .iter()
                    .cloned()
                    .zip(args.iter().map(|a| (*a).clone()))
                    .collect();
                visiting.push(n.clone());
                let r = d.cons.iter().any(|c| {
                    self.cons.get(c).is_none_or(|ci| {
                        ci.fields
                            .iter()
                            .any(|f| self.closure_in(&f.subst(&sub), visiting))
                    })
                });
                visiting.pop();
                r
            }
        }
    }

    fn is_ltype_in(&self, t: &Ty, visiting: &mut Vec<String>) -> bool {
        match t {
            Ty::Var(v) => self.ltype_vars.contains(v),
            Ty::Fun(..) | Ty::Forall(..) | Ty::Meta(_) => false,
            Ty::Con(_) | Ty::App(..) => {
                let (h, args) = t.spine();
                let Ty::Con(n) = h else {
                    return false;
                };
                if let Some(&k) = self.ltype_datas.get(n) {
                    return args.len() >= k;
                }
                if visiting.contains(n) {
                    return false;
                }
                let Some(d) = self.datas.get(n) else {
                    return false;
                };
                if args.len() != d.params.len() {
                    return false;
                }
                let sub: Vec<(String, Ty)> = d
                    .params
                    .iter()
                    .cloned()
                    .zip(args.iter().map(|a| (*a).clone()))
                    .collect();
                visiting.push(n.clone());
                let r = d.cons.iter().any(|c| {
                    self.cons.get(c).is_some_and(|ci| {
                        ci.fields
                            .iter()
                            .any(|f| self.is_ltype_in(&f.subst(&sub), visiting))
                    })
                });
                visiting.pop();
                r
            }
        }
    }

    fn lit(&mut self, l: &ast::Lit, sp: Span) -> (tast::Expr, Ty) {
        match l {
            ast::Lit::Int(n) => {
                if let Some(c) = self.overloaded("Prelude.Num", "Prelude.fromInteger") {
                    let t = self.fresh();
                    let h = self.want(Pred::new(&c, t.clone()), sp);
                    let l = match i64::try_from(*n) {
                        Ok(n) => tast::Lit::Int(n),
                        Err(_) => tast::Lit::BigInt(*n),
                    };
                    return (tast::Expr::OverLit(l, t.clone(), h), t);
                }
                (
                    tast::Expr::Lit(tast::Lit::Int(*n as i64)),
                    self.lit_ty("Int"),
                )
            }
            ast::Lit::Frac(x) => {
                if let Some(c) = self.overloaded("Prelude.Fractional", "Prelude.fromRational") {
                    let t = self.fresh();
                    let h = self.want(Pred::new(&c, t.clone()), sp);
                    return (tast::Expr::OverLit(tast::Lit::Double(*x), t.clone(), h), t);
                }
                (
                    tast::Expr::Lit(tast::Lit::Double(*x)),
                    self.lit_ty("Double"),
                )
            }
            ast::Lit::Char(c) => (tast::Expr::Lit(tast::Lit::Char(*c)), self.lit_ty("Char")),
            ast::Lit::Str(s) => {
                let c = self.lit_ty("Char");
                (tast::Expr::Lit(tast::Lit::Str(s.clone())), Ty::list(c))
            }
        }
    }

    /// The class of overloaded literals when the Prelude defines it.
    fn overloaded(&self, class: &str, method: &str) -> Option<String> {
        let class = self.wired(class);
        (self.classes.contains_key(&class) && self.globals.contains_key(&self.wired(method)))
            .then_some(class)
    }

    fn list_lit(&mut self, items: Vec<tast::Expr>, a: &Ty) -> tast::Expr {
        let nil = tast::Expr::Con(list_tc(), vec![a.clone()]);
        items.into_iter().rev().fold(nil, |acc, x| {
            tast::Expr::App(
                Box::new(tast::Expr::App(
                    Box::new(tast::Expr::Con(prim(":"), vec![a.clone()])),
                    Box::new(x),
                )),
                Box::new(acc),
            )
        })
    }

    fn con_pat0(&self, name: &str) -> tast::Pat {
        tast::Pat::Con(prim(name), Vec::new(), Vec::new())
    }

    fn let_mono(&mut self, v: &str, t: Ty, rhs: tast::Expr, body: tast::Expr) -> tast::Expr {
        tast::Expr::Let(
            vec![BindGroup::NonRec(tast::Bind {
                name: v.to_string(),
                tvs: Vec::new(),
                ty: t,
                rhs,
                span: Span::DUMMY,
                strict: false,
            })],
            Box::new(body),
        )
    }

    /// `case scrut of { p1 -> e1; ... }` with already typed parts.
    fn match_on(
        &mut self,
        scrut: tast::Expr,
        st: Ty,
        alts: Vec<(tast::Pat, tast::Expr)>,
        res: Ty,
        sp: Span,
    ) -> tast::Expr {
        let v = self.fresh_name("$c");
        let clauses = alts
            .into_iter()
            .map(|(p, e)| Clause {
                pats: vec![p],
                rhs: Rhs {
                    wheres: Vec::new(),
                    body: vec![(Vec::new(), e)],
                },
                span: sp,
            })
            .collect();
        let m = tast::Expr::Match(vec![(v.clone(), st.clone())], clauses, res);
        self.let_mono(&v, st, scrut, m)
    }

    fn alts(&mut self, alts: &[ast::Alt], st: &Ty, res: &Ty) -> Vec<Clause> {
        alts.iter()
            .map(|a| {
                let p = self.pat(&a.pat, st);
                let rhs = self.rhs(&a.rhs, res);
                Clause {
                    pats: vec![p],
                    rhs,
                    span: a.span,
                }
            })
            .collect()
    }

    fn do_stmts(&mut self, ss: &[Stmt], sp: Span) -> (tast::Expr, Ty) {
        let Some((s, rest)) = ss.split_first() else {
            self.error(err("E0102", sp, "empty `do` block"));
            let t = self.fresh();
            return (tast::Expr::Error("do".into(), t.clone()), t);
        };
        if rest.is_empty() {
            return match s {
                Stmt::Expr(e) => {
                    let (te, t) = self.infer(e, None);
                    (self.mark(e.span, te, &t), t)
                }
                _ => {
                    let t = self.fresh();
                    (tast::Expr::Error("do".into(), t.clone()), t)
                }
            };
        }
        match s {
            Stmt::Expr(e) => {
                let then = self.sugar("Prelude.>>", e.span);
                let a = self.fresh();
                let te = self.expr(e, &a);
                let f = self.app(e.span, then, te, &a);
                let (r, rt) = self.do_stmts(rest, sp);
                self.app(sp, f, r, &rt)
            }
            Stmt::Bind(p, e, bsp) => {
                let bind = self.sugar("Prelude.>>=", *bsp);
                let a = self.fresh();
                let te = self.expr(e, &a);
                let f = self.app(e.span, bind, te, &a);
                // The bound value's type is the argument of the monad.
                let x = self.fresh();
                let m = self.fresh();
                self.unify(e.span, &Ty::App(Box::new(m), Box::new(x.clone())), &a);
                let tp = self.pat(p, &x);
                let (r, rt) = self.do_stmts(rest, sp);
                let v = self.fresh_name("$d");
                let body = if let tast::Pat::Var(n, t) = &tp {
                    tast::Expr::Lam(n.clone(), t.clone(), Box::new(r))
                } else {
                    let msg = "Pattern match failure in do expression".to_string();
                    let fail_fn = self.wired("Prelude.fail");
                    let fail = if self.failable(&tp) && self.globals.contains_key(&fail_fn) {
                        // `fail "..."` of `MonadFail` (Haskell 2010 §3.14 with
                        // the MonadFail proposal).
                        let (f, ft) = self.sugar("Prelude.fail", *bsp);
                        let s = tast::Expr::Lit(tast::Lit::Str(msg));
                        let st = Ty::list(Ty::Con(prim("Char")));
                        let (e, et) = self.app(*bsp, (f, ft), s, &st);
                        self.unify(*bsp, &et, &rt);
                        e
                    } else {
                        tast::Expr::Error(msg, rt.clone())
                    };
                    let m = tast::Expr::Match(
                        vec![(v.clone(), x.clone())],
                        vec![
                            Clause {
                                pats: vec![tp],
                                rhs: Rhs {
                                    wheres: Vec::new(),
                                    body: vec![(Vec::new(), r)],
                                },
                                span: *bsp,
                            },
                            Clause {
                                pats: vec![tast::Pat::Wild(x.clone())],
                                rhs: Rhs {
                                    wheres: Vec::new(),
                                    body: vec![(Vec::new(), fail)],
                                },
                                span: Span::DUMMY,
                            },
                        ],
                        rt.clone(),
                    );
                    tast::Expr::Lam(v, x.clone(), Box::new(m))
                };
                let kt = Ty::fun(x, rt);
                self.app(sp, f, body, &kt)
            }
            Stmt::Let(ds, _) => {
                let gs = self.local_decls(ds);
                let (r, rt) = self.do_stmts(rest, sp);
                (tast::Expr::Let(gs, Box::new(r)), rt)
            }
        }
    }

    /// `[h | qs]` with element type `a`.
    fn list_comp(&mut self, h: &ast::Expr, qs: &[Stmt], a: &Ty, sp: Span) -> tast::Expr {
        let Some((q, rest)) = qs.split_first() else {
            let th = self.expr(h, a);
            return self.list_lit(vec![th], a);
        };
        match q {
            Stmt::Expr(c) => {
                let b = self.bool_ty();
                let tc = self.expr(c, &b);
                let inner = self.list_comp(h, rest, a, sp);
                let nil = tast::Expr::Con(list_tc(), vec![a.clone()]);
                let la = Ty::list(a.clone());
                self.match_on(
                    tc,
                    b,
                    vec![
                        (self.con_pat0("True"), inner),
                        (self.con_pat0("False"), nil),
                    ],
                    la,
                    sp,
                )
            }
            Stmt::Let(ds, _) => {
                let gs = self.local_decls(ds);
                let inner = self.list_comp(h, rest, a, sp);
                tast::Expr::Let(gs, Box::new(inner))
            }
            Stmt::Bind(p, l, bsp) => {
                // concatMap (\x -> case x of { p -> [h | rest]; _ -> [] }) l
                let x = self.fresh();
                let tl = self.expr(l, &Ty::list(x.clone()));
                let tp = self.pat(p, &x);
                let inner = self.list_comp(h, rest, a, sp);
                let la = Ty::list(a.clone());
                let v = self.fresh_name("$q");
                let m = tast::Expr::Match(
                    vec![(v.clone(), x.clone())],
                    vec![
                        Clause {
                            pats: vec![tp],
                            rhs: Rhs {
                                wheres: Vec::new(),
                                body: vec![(Vec::new(), inner)],
                            },
                            span: *bsp,
                        },
                        Clause {
                            pats: vec![tast::Pat::Wild(x.clone())],
                            rhs: Rhs {
                                wheres: Vec::new(),
                                body: vec![(
                                    Vec::new(),
                                    tast::Expr::Con(list_tc(), vec![a.clone()]),
                                )],
                            },
                            // Generated: never reported as redundant.
                            span: Span::DUMMY,
                        },
                    ],
                    la.clone(),
                );
                let lam = tast::Expr::Lam(v, x.clone(), Box::new(m));
                let cm = self.sugar("Prelude.concatMap", *bsp);
                let f = self.app(*bsp, cm, lam, &Ty::fun(x.clone(), la.clone()));
                let (r, rt) = self.app(*bsp, f, tl, &Ty::list(x));
                self.unify(sp, &la, &rt);
                r
            }
        }
    }

    /// `e :: t`: a signature with type variables makes a local polymorphic
    /// binding.
    fn typed(&mut self, x: &ast::Expr, t: &ast::Type, sp: Span) -> (tast::Expr, Ty) {
        let s = self.sig_scheme(t);
        if s.vars.is_empty() {
            let te = self.expr(x, &s.ty);
            return (te, s.ty);
        }
        let name = self.fresh_name("$t");
        let te = self.expr(x, &s.ty);
        let tvs = s
            .vars
            .iter()
            .map(|v| (v.clone(), kind_of(v, &s.ty)))
            .collect();
        let (ty, inst) = self.instantiate(&s);
        let e = tast::Expr::Let(
            vec![BindGroup::NonRec(tast::Bind {
                name: name.clone(),
                tvs,
                ty: s.ty.clone(),
                rhs: te,
                span: sp,
                strict: false,
            })],
            Box::new(tast::Expr::Global(name, inst)),
        );
        (e, ty)
    }

    fn record_con(
        &mut self,
        c: &ast::QName,
        fs: &[(ast::QName, Option<ast::Expr>)],
        sp: Span,
    ) -> (tast::Expr, Ty) {
        let name = c.text();
        let Some(ci) = self.cons.get(&name).cloned() else {
            self.error(err(
                "E0201",
                c.span,
                format!("constructor `{}` is not defined", short(&name)),
            ));
            let t = self.fresh();
            return (tast::Expr::Error(name, t.clone()), t);
        };
        let (mut f, mut ft) = self.con_expr(&name);
        let mut given: Vec<Option<&ast::Expr>> = vec![None; ci.fields.len()];
        for (fname, v) in fs {
            let fq = fname.text();
            match ci
                .field_names
                .iter()
                .position(|n| n.as_deref() == Some(&fq))
            {
                Some(i) => given[i] = v.as_ref(),
                None => self.error(err(
                    "E0201",
                    fname.span,
                    format!(
                        "constructor `{}` does not have field `{}`",
                        short(&name),
                        short(&fq)
                    ),
                )),
            }
        }
        let named = |i: usize| ci.field_names[i].as_deref().map(short).unwrap_or("?");
        let missing: Vec<usize> = (0..ci.fields.len())
            .filter(|&i| given[i].is_none() && ci.field_names[i].is_some())
            .collect();
        let strict: Vec<&str> = missing
            .iter()
            .filter(|&&i| ci.strict[i])
            .map(|&i| named(i))
            .collect();
        if !strict.is_empty() {
            self.error(
                err(
                    "E0616",
                    sp,
                    format!(
                        "constructor `{}` does not have the required strict field(s): {}",
                        short(&name),
                        strict.join(", ")
                    ),
                )
                .with_note("a strict field must be given when the record is constructed"),
            );
        } else if !missing.is_empty() {
            let names: Vec<String> = missing.iter().map(|&i| format!("`{}`", named(i))).collect();
            self.out.warnings.push(
                Diagnostic::warning(format!(
                    "fields of `{}` not initialised: {}",
                    short(&name),
                    names.join(", ")
                ))
                .with_code("W0617")
                .with_label(sp, "")
                .with_note("using a missing field is a runtime error"),
            );
        }
        for (i, g) in given.into_iter().enumerate() {
            let a = self.fresh();
            let ta = match g {
                Some(x) => self.expr(x, &a),
                None => tast::Expr::Error(
                    self.located(
                        sp,
                        &format!("Missing field in record construction {}", named(i)),
                    ),
                    a.clone(),
                ),
            };
            (f, ft) = self.app(sp, (f, ft), ta, &a);
        }
        (f, ft)
    }

    fn record_upd(
        &mut self,
        x: &ast::Expr,
        fs: &[(ast::QName, ast::Expr)],
        sp: Span,
    ) -> (tast::Expr, Ty) {
        let names: Vec<String> = fs.iter().map(|(f, _)| f.text()).collect();
        let Some(first) = names.first().and_then(|n| self.fields.get(n)).cloned() else {
            self.error(err("E0201", sp, "record update with an unknown field"));
            let t = self.fresh();
            return (tast::Expr::Error("upd".into(), t.clone()), t);
        };
        let data = self.cons[&first[0].0].data.clone();
        let di = self.datas[&data].clone();
        let inst: Vec<Ty> = di.params.iter().map(|_| self.fresh()).collect();
        let sub: Vec<(String, Ty)> = di
            .params
            .iter()
            .cloned()
            .zip(inst.iter().cloned())
            .collect();
        let rt = Ty::con(&data, inst.clone());
        let tx = self.expr(x, &rt);
        let mut vals = Vec::new();
        for (f, v) in fs {
            let fq = f.text();
            let Some((c0, i0)) = self.fields.get(&fq).and_then(|v| v.first()).cloned() else {
                self.error(err(
                    "E0201",
                    f.span,
                    format!("`{}` is not a record field", short(&fq)),
                ));
                continue;
            };
            if self.cons[&c0].data != data {
                self.error(err(
                    "E0601",
                    f.span,
                    format!(
                        "field `{}` does not belong to type `{}`",
                        short(&fq),
                        short(&data)
                    ),
                ));
                continue;
            }
            let ft = self.cons[&c0].fields[i0].subst(&sub);
            vals.push((fq, self.expr(v, &ft)));
        }
        let mut clauses = Vec::new();
        let mut all = true;
        for c in &di.cons {
            let ci = self.cons[c].clone();
            if !names
                .iter()
                .all(|n| ci.field_names.iter().any(|f| f.as_deref() == Some(n)))
            {
                all = false;
                continue;
            }
            let vars: Vec<(String, Ty)> = ci
                .fields
                .iter()
                .map(|t| (self.fresh_name("$u"), t.subst(&sub)))
                .collect();
            let pats = vars
                .iter()
                .map(|(v, t)| tast::Pat::Var(v.clone(), t.clone()))
                .collect();
            let mut e = tast::Expr::Con(c.clone(), inst.clone());
            for (k, (v, t)) in vars.iter().enumerate() {
                let field = ci.field_names[k].clone();
                let val = vals
                    .iter()
                    .find(|(n, _)| Some(n) == field.as_ref())
                    .map(|x| x.1.clone())
                    .unwrap_or_else(|| tast::Expr::Local(v.clone(), t.clone()));
                e = tast::Expr::App(Box::new(e), Box::new(val));
            }
            clauses.push(Clause {
                pats: vec![tast::Pat::Con(c.clone(), inst.clone(), pats)],
                rhs: Rhs {
                    wheres: Vec::new(),
                    body: vec![(Vec::new(), e)],
                },
                span: sp,
            });
        }
        if clauses.is_empty() && vals.len() == fs.len() {
            let shown: Vec<String> = names.iter().map(|n| format!("`{}`", short(n))).collect();
            self.error(
                err(
                    "E0601",
                    sp,
                    format!("no constructor has all these fields: {}", shown.join(", ")),
                )
                .with_note("the fields of one record update must belong to one constructor"),
            );
        }
        if !all {
            clauses.push(Clause {
                pats: vec![tast::Pat::Wild(rt.clone())],
                rhs: Rhs {
                    wheres: Vec::new(),
                    body: vec![(
                        Vec::new(),
                        tast::Expr::Error(
                            self.located(sp, "Non-exhaustive patterns in record update"),
                            rt.clone(),
                        ),
                    )],
                },
                span: Span::DUMMY,
            });
        }
        let v = self.fresh_name("$c");
        let m = tast::Expr::Match(vec![(v.clone(), rt.clone())], clauses, rt.clone());
        (self.let_mono(&v, rt.clone(), tx, m), rt)
    }

    // ---- patterns ----

    /// The pattern of an overloaded literal: `(== lit)` with an `Eq`
    /// dictionary (Haskell 2010 §3.17.2), or a plain literal pattern once
    /// the type is known to be `Int` / `Double`.
    fn overlit_pat(&mut self, l: tast::Lit, h: u32, t: &Ty, sp: Span) -> tast::Pat {
        let eq = self.want(Pred::new(&self.wired("Prelude.Eq"), t.clone()), sp);
        let x = format!("lit${}", self.holes);
        let test = tast::Expr::Lam(
            x.clone(),
            t.clone(),
            Box::new(app_all(
                tast::Expr::Global(self.wired("Prelude.=="), vec![t.clone()]),
                vec![
                    tast::Expr::Hole(eq),
                    tast::Expr::Local(x, t.clone()),
                    tast::Expr::OverLit(l.clone(), t.clone(), h),
                ],
            )),
        );
        tast::Pat::OverLit(l, Box::new(test), t.clone())
    }

    pub fn pat(&mut self, p: &ast::Pat, t: &Ty) -> tast::Pat {
        let sp = p.span;
        if self.has_ltype {
            self.pat_tys.insert(sp, t.clone());
        }
        match &p.kind {
            PatKind::Var(v) => {
                self.locals
                    .insert(v.name.clone(), LocalKind::Mono(t.clone()));
                tast::Pat::Var(v.name.clone(), t.clone())
            }
            PatKind::Wild | PatKind::Error => tast::Pat::Wild(t.clone()),
            PatKind::Lit(l) => {
                let (e, lt) = self.lit(l, sp);
                self.unify(sp, t, &lt);
                match e {
                    tast::Expr::Lit(l) => tast::Pat::Lit(l, t.clone()),
                    tast::Expr::OverLit(l, _, h) => self.overlit_pat(l, h, t, sp),
                    _ => tast::Pat::Wild(t.clone()),
                }
            }
            PatKind::NegLit(l) => {
                let (e, lt) = self.lit(l, sp);
                self.unify(sp, t, &lt);
                match e {
                    tast::Expr::OverLit(tast::Lit::Int(n), _, h) => {
                        self.overlit_pat(tast::Lit::Int(-n), h, t, sp)
                    }
                    tast::Expr::OverLit(tast::Lit::Double(x), _, h) => {
                        self.overlit_pat(tast::Lit::Double(-x), h, t, sp)
                    }
                    tast::Expr::Lit(tast::Lit::Int(n)) => {
                        tast::Pat::Lit(tast::Lit::Int(-n), t.clone())
                    }
                    tast::Expr::Lit(tast::Lit::Double(x)) => {
                        tast::Pat::Lit(tast::Lit::Double(-x), t.clone())
                    }
                    _ => tast::Pat::Wild(t.clone()),
                }
            }
            PatKind::Con(c, ps) => {
                let name = c.text();
                self.con_pat(&name, ps.iter().collect(), t, sp, c.span)
            }
            PatKind::Tuple(ps) => {
                let tc = self.ensure_tuple(ps.len());
                self.con_pat(&tc, ps.iter().collect(), t, sp, sp)
            }
            PatKind::List(ps) => {
                let a = self.fresh();
                self.unify(sp, t, &Ty::list(a.clone()));
                let mut acc = tast::Pat::Con(list_tc(), vec![a.clone()], Vec::new());
                let items: Vec<tast::Pat> = ps.iter().map(|q| self.pat(q, &a)).collect();
                for q in items.into_iter().rev() {
                    acc = tast::Pat::Con(prim(":"), vec![a.clone()], vec![q, acc]);
                }
                acc
            }
            PatKind::As(v, q) => {
                self.locals
                    .insert(v.name.clone(), LocalKind::Mono(t.clone()));
                let tq = self.pat(q, t);
                tast::Pat::As(v.name.clone(), t.clone(), Box::new(tq))
            }
            PatKind::Lazy(q) => tast::Pat::Lazy(Box::new(self.pat(q, t))),
            PatKind::Bang(q) => tast::Pat::Bang(Box::new(self.pat(q, t))),
            PatKind::Record(c, fs) => {
                let name = c.text();
                let Some(ci) = self.cons.get(&name).cloned() else {
                    self.error(err(
                        "E0201",
                        c.span,
                        format!("constructor `{}` is not defined", short(&name)),
                    ));
                    return tast::Pat::Wild(t.clone());
                };
                let wild = ast::Pat {
                    kind: PatKind::Wild,
                    span: sp,
                };
                let mut args: Vec<&ast::Pat> = vec![&wild; ci.fields.len()];
                for (f, q) in fs {
                    let fq = f.text();
                    match ci
                        .field_names
                        .iter()
                        .position(|n| n.as_deref() == Some(&fq))
                    {
                        Some(i) => {
                            if let Some(q) = q {
                                args[i] = q;
                            }
                        }
                        None => self.error(err(
                            "E0201",
                            f.span,
                            format!(
                                "constructor `{}` does not have field `{}`",
                                short(&name),
                                short(&fq)
                            ),
                        )),
                    }
                }
                self.con_pat(&name, args, t, sp, c.span)
            }
            PatKind::Typed(q, ty) => {
                let s = self.sig_scheme(ty);
                let (at, _) = self.instantiate(&s);
                self.unify(sp, t, &at);
                self.pat(q, t)
            }
            PatKind::Paren(q) => self.pat(q, t),
            PatKind::OpSeq(_) => tast::Pat::Wild(t.clone()),
        }
    }

    fn con_pat(
        &mut self,
        name: &str,
        ps: Vec<&ast::Pat>,
        t: &Ty,
        sp: Span,
        csp: Span,
    ) -> tast::Pat {
        if let Some(n) = tuple_arity(name) {
            self.ensure_tuple(n);
        }
        let Some(ci) = self.cons.get(name).cloned() else {
            self.error(err(
                "E0201",
                csp,
                format!("constructor `{}` is not defined", short(name)),
            ));
            return tast::Pat::Wild(t.clone());
        };
        if ci.fields.len() != ps.len() {
            self.error(err(
                "E0601",
                sp,
                format!(
                    "the constructor `{}` should have {} arguments, but has been given {}",
                    short(name),
                    ci.fields.len(),
                    ps.len()
                ),
            ));
            for p in ps {
                let a = self.fresh();
                self.pat(p, &a);
            }
            return tast::Pat::Wild(t.clone());
        }
        let di = self.datas[&ci.data].clone();
        let inst: Vec<Ty> = di.params.iter().map(|_| self.fresh()).collect();
        let sub: Vec<(String, Ty)> = di
            .params
            .iter()
            .cloned()
            .zip(inst.iter().cloned())
            .collect();
        self.unify(sp, t, &Ty::con(&ci.data, inst.clone()));
        let args = ps
            .iter()
            .zip(&ci.fields)
            .map(|(p, f)| {
                let ft = f.subst(&sub);
                self.pat(p, &ft)
            })
            .collect();
        tast::Pat::Con(name.to_string(), inst, args)
    }

    // ---- final pass ----

    /// Replaces metas by their solutions (unsolved ones default to `()`)
    /// and fills in recursive instantiations.
    pub fn finish_expr(&self, e: &mut tast::Expr) {
        match e {
            tast::Expr::Local(_, t) | tast::Expr::Error(_, t) => *t = self.final_ty(t),
            tast::Expr::Hole(h) => {
                let mut d = match self.dict_sol.get(h) {
                    Some(d) => d.clone(),
                    None => {
                        tast::Expr::Error("unsolved class constraint".into(), Ty::Con(unit_tc()))
                    }
                };
                self.finish_expr(&mut d);
                *e = d;
            }
            tast::Expr::OverLit(l, t, h) => {
                let t = self.final_ty(t);
                let int = Ty::Con(prim("Int"));
                let dbl = Ty::Con(prim("Double"));
                let lit = |l: tast::Lit| tast::Expr::Lit(l);
                *e = match (&*l, &t) {
                    (tast::Lit::Int(n), t) if *t == int => lit(tast::Lit::Int(*n)),
                    (tast::Lit::BigInt(n), t) if *t == int => lit(tast::Lit::Int(*n as i64)),
                    (tast::Lit::Int(n), t) if *t == dbl => lit(tast::Lit::Double(*n as f64)),
                    (tast::Lit::BigInt(n), t) if *t == dbl => lit(tast::Lit::Double(*n as f64)),
                    (tast::Lit::Double(x), t) if *t == dbl => lit(tast::Lit::Double(*x)),
                    (tast::Lit::Int(_) | tast::Lit::BigInt(_), _) => {
                        let n = match &*l {
                            tast::Lit::Int(n) => i128::from(*n),
                            tast::Lit::BigInt(n) => *n as i128,
                            _ => unreachable!(),
                        };
                        let mut d = tast::Expr::Hole(*h);
                        self.finish_expr(&mut d);
                        let f =
                            tast::Expr::Global(self.wired("Prelude.fromInteger"), vec![t.clone()]);
                        let arg = self.integer_lit(n);
                        tast::Expr::App(
                            Box::new(tast::Expr::App(Box::new(f), Box::new(d))),
                            Box::new(arg),
                        )
                    }
                    (tast::Lit::Double(x), _) => {
                        let mut d = tast::Expr::Hole(*h);
                        self.finish_expr(&mut d);
                        let (n, q) = rational_digits(*x);
                        let it = self.integer_ty();
                        let (n, q) = (self.integer_lit_str(&n), self.integer_lit_str(&q));
                        let ratio = tast::Expr::App(
                            Box::new(tast::Expr::App(
                                Box::new(tast::Expr::Con(self.wired("Prelude.:%"), vec![it])),
                                Box::new(n),
                            )),
                            Box::new(q),
                        );
                        let f =
                            tast::Expr::Global(self.wired("Prelude.fromRational"), vec![t.clone()]);
                        tast::Expr::App(
                            Box::new(tast::Expr::App(Box::new(f), Box::new(d))),
                            Box::new(ratio),
                        )
                    }
                    (l, _) => lit(l.clone()),
                };
            }
            tast::Expr::Global(n, inst) => {
                if inst.len() == 1 && inst[0] == Ty::Con(REC.into()) {
                    *inst = self
                        .rec_tvs
                        .get(n.as_str())
                        .map(|vs| vs.iter().map(|v| Ty::Var(v.clone())).collect())
                        .unwrap_or_default();
                    if let Some(ds) = self.rec_dicts.get(n.as_str())
                        && !ds.is_empty()
                    {
                        let mut x = e.clone();
                        for (d, t) in ds {
                            x = tast::Expr::App(
                                Box::new(x),
                                Box::new(tast::Expr::Local(d.clone(), t.clone())),
                            );
                        }
                        *e = x;
                    }
                } else {
                    for t in inst.iter_mut() {
                        *t = self.final_ty(t);
                    }
                }
            }
            tast::Expr::Con(_, inst) => {
                for t in inst.iter_mut() {
                    *t = self.final_ty(t);
                }
            }
            tast::Expr::Lit(_) => {}
            tast::Expr::App(f, a) => {
                self.finish_expr(f);
                self.finish_expr(a);
            }
            tast::Expr::Coerce(x, t) | tast::Expr::Mark(_, x, t) => {
                *t = self.final_ty(t);
                self.finish_expr(x);
            }
            tast::Expr::Lam(_, t, b) => {
                *t = self.final_ty(t);
                self.finish_expr(b);
            }
            tast::Expr::Let(gs, b) => {
                for g in gs.iter_mut() {
                    self.finish_group(g);
                }
                self.finish_expr(b);
            }
            tast::Expr::Match(vs, cls, t) => {
                for (_, vt) in vs.iter_mut() {
                    *vt = self.final_ty(vt);
                }
                for c in cls.iter_mut() {
                    for p in c.pats.iter_mut() {
                        self.finish_pat(p);
                    }
                    self.finish_rhs(&mut c.rhs);
                }
                *t = self.final_ty(t);
            }
        }
    }

    fn finish_rhs(&self, r: &mut Rhs) {
        for g in r.wheres.iter_mut() {
            self.finish_group(g);
        }
        for (gs, e) in r.body.iter_mut() {
            for g in gs.iter_mut() {
                match g {
                    Guard::Bool(e) => self.finish_expr(e),
                    Guard::Bind(p, e) => {
                        self.finish_pat(p);
                        self.finish_expr(e);
                    }
                    Guard::Let(bs) => {
                        for b in bs.iter_mut() {
                            self.finish_group(b);
                        }
                    }
                }
            }
            self.finish_expr(e);
        }
    }

    pub fn finish_group(&self, g: &mut BindGroup) {
        let bs: &mut [tast::Bind] = match g {
            BindGroup::NonRec(b) => std::slice::from_mut(b),
            BindGroup::Rec(bs) => bs,
        };
        for b in bs {
            b.ty = self.final_ty(&b.ty);
            self.finish_expr(&mut b.rhs);
        }
    }

    fn finish_pat(&self, p: &mut tast::Pat) {
        match p {
            tast::Pat::Var(_, t) | tast::Pat::Wild(t) | tast::Pat::Lit(_, t) => {
                *t = self.final_ty(t)
            }
            tast::Pat::Con(_, inst, ps) => {
                for t in inst.iter_mut() {
                    *t = self.final_ty(t);
                }
                for q in ps.iter_mut() {
                    self.finish_pat(q);
                }
            }
            tast::Pat::As(_, t, q) => {
                *t = self.final_ty(t);
                self.finish_pat(q);
            }
            tast::Pat::Lazy(q) | tast::Pat::Bang(q) => self.finish_pat(q),
            tast::Pat::OverLit(l, test, t) => {
                let ft = self.final_ty(t);
                let lit = match (&*l, &ft) {
                    (tast::Lit::Int(n), Ty::Con(c)) if *c == prim("Int") => {
                        Some(tast::Lit::Int(*n))
                    }
                    (tast::Lit::BigInt(n), Ty::Con(c)) if *c == prim("Int") => {
                        Some(tast::Lit::Int(*n as i64))
                    }
                    (tast::Lit::Int(n), Ty::Con(c)) if *c == prim("Double") => {
                        Some(tast::Lit::Double(*n as f64))
                    }
                    (tast::Lit::Double(x), Ty::Con(c)) if *c == prim("Double") => {
                        Some(tast::Lit::Double(*x))
                    }
                    _ => None,
                };
                match lit {
                    Some(l) => *p = tast::Pat::Lit(l, ft),
                    None => {
                        *t = ft;
                        self.finish_expr(test);
                    }
                }
            }
        }
    }

    pub fn final_ty(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::Meta(_) => Ty::Con(unit_tc()),
            Ty::App(f, a) => Ty::App(Box::new(self.final_ty(&f)), Box::new(self.final_ty(&a))),
            Ty::Fun(a, b) => Ty::Fun(Box::new(self.final_ty(&a)), Box::new(self.final_ty(&b))),
            Ty::Forall(vs, b) => Ty::Forall(vs, Box::new(self.final_ty(&b))),
            t => t,
        }
    }
}

/// `x` as a fraction `n / d` in lowest terms, from its shortest decimal
/// representation (`0.1` is `1/10`, as the literal is written), as decimal
/// strings of any size (`1e300`, `5e-324`); the numerator carries the sign.
pub(crate) fn rational_digits(x: f64) -> (String, String) {
    if !x.is_finite() {
        // Not produced by the lexer; keep something well formed.
        return ("0".into(), "1".into());
    }
    let s = format!("{x:e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i64 = exp.parse().unwrap_or(0);
    let neg = mant.starts_with('-');
    let mant = mant.trim_start_matches('-');
    let (ip, fp) = mant.split_once('.').unwrap_or((mant, ""));
    let mut n: u64 = format!("{ip}{fp}").parse().unwrap_or(0);
    let scale = exp - fp.len() as i64;
    let sign = if neg && n != 0 { "-" } else { "" };
    if n == 0 {
        return ("0".into(), "1".into());
    }
    if scale >= 0 {
        let mut v = decimal_mul_small(&n.to_string(), 1);
        for _ in 0..scale {
            v.push('0');
        }
        return (format!("{sign}{v}"), "1".into());
    }
    // n / (2^k * 5^k): cancel the factors 2 and 5 of n.
    let (mut twos, mut fives) = (-scale, -scale);
    while twos > 0 && n.is_multiple_of(2) {
        n /= 2;
        twos -= 1;
    }
    while fives > 0 && n.is_multiple_of(5) {
        n /= 5;
        fives -= 1;
    }
    let mut d = String::from("1");
    for _ in 0..twos {
        d = decimal_mul_small(&d, 2);
    }
    for _ in 0..fives {
        d = decimal_mul_small(&d, 5);
    }
    (format!("{sign}{n}"), d)
}

/// A non-negative decimal string times a small factor.
fn decimal_mul_small(s: &str, k: u32) -> String {
    let mut out = Vec::with_capacity(s.len() + 2);
    let mut carry = 0u32;
    for c in s.bytes().rev() {
        let v = u32::from(c - b'0') * k + carry;
        out.push(b'0' + (v % 10) as u8);
        carry = v / 10;
    }
    while carry > 0 {
        out.push(b'0' + (carry % 10) as u8);
        carry /= 10;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `a`, `b`, ..., `z`, `a1`, ...
fn tv_letter(i: usize) -> String {
    let c = (b'a' + (i % 26) as u8) as char;
    if i < 26 {
        c.to_string()
    } else {
        format!("{c}{}", i / 26)
    }
}

pub(crate) enum UErr {
    Mismatch,
    Rigid(String),
    Occurs,
}

/// The kind of type variable `v` in `t`, from how many arguments it is
/// applied to.
pub fn kind_of(v: &str, t: &Ty) -> Kind {
    applied_arity(v, t).map(Kind::arity).unwrap_or(Kind::Star)
}

pub(crate) fn applied_arity(v: &str, t: &Ty) -> Option<usize> {
    match t {
        Ty::App(..) => {
            let (h, args) = t.spine();
            let here = match h {
                Ty::Var(x) if x == v => Some(args.len()),
                _ => None,
            };
            here.or_else(|| args.iter().find_map(|a| applied_arity(v, a)))
        }
        Ty::Fun(a, b) => applied_arity(v, a).or_else(|| applied_arity(v, b)),
        Ty::Forall(_, b) => applied_arity(v, b),
        _ => None,
    }
}

fn tpat_vars(p: &tast::Pat, out: &mut Vec<(String, Ty)>) {
    match p {
        tast::Pat::Var(v, t) => out.push((v.clone(), t.clone())),
        tast::Pat::As(v, t, q) => {
            out.push((v.clone(), t.clone()));
            tpat_vars(q, out);
        }
        tast::Pat::Con(_, _, ps) => {
            for q in ps {
                tpat_vars(q, out);
            }
        }
        tast::Pat::Lazy(q) | tast::Pat::Bang(q) => tpat_vars(q, out),
        _ => {}
    }
}

fn achc_rename_pat_vars(p: &ast::Pat, out: &mut Vec<String>) {
    match &p.kind {
        PatKind::Var(v) => out.push(v.name.clone()),
        PatKind::As(v, q) => {
            out.push(v.name.clone());
            achc_rename_pat_vars(q, out);
        }
        PatKind::Con(_, ps) | PatKind::Tuple(ps) | PatKind::List(ps) => {
            for q in ps {
                achc_rename_pat_vars(q, out);
            }
        }
        PatKind::Lazy(q) | PatKind::Bang(q) | PatKind::Paren(q) | PatKind::Typed(q, _) => {
            achc_rename_pat_vars(q, out)
        }
        PatKind::Record(_, fs) => {
            for (_, q) in fs {
                if let Some(q) = q {
                    achc_rename_pat_vars(q, out);
                }
            }
        }
        _ => {}
    }
}

// ---- free variables for dependency analysis ----

fn decl_free_vars(d: &Decl, out: &mut HashSet<String>) {
    match d {
        Decl::Fun(_, ps, rhs, _) => {
            for p in ps {
                pat_free_vars(p, out);
            }
            rhs_free_vars(rhs, out);
        }
        Decl::PatBind(p, rhs, _) => {
            pat_free_vars(p, out);
            rhs_free_vars(rhs, out);
        }
        _ => {}
    }
}

fn pat_free_vars(_p: &ast::Pat, _out: &mut HashSet<String>) {}

fn rhs_free_vars(r: &ast::Rhs, out: &mut HashSet<String>) {
    for d in &r.wheres {
        decl_free_vars(d, out);
    }
    match &r.body {
        ast::RhsBody::Plain(e) => expr_free_vars(e, out),
        ast::RhsBody::Guarded(gs) => {
            for g in gs {
                for s in &g.guards {
                    stmt_free_vars(s, out);
                }
                expr_free_vars(&g.body, out);
            }
        }
    }
}

fn stmt_free_vars(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Expr(e) | Stmt::Bind(_, e, _) => expr_free_vars(e, out),
        Stmt::Let(ds, _) => {
            for d in ds {
                decl_free_vars(d, out);
            }
        }
    }
}

/// Whether an expression is a (partially applied) data constructor.
fn con_headed(e: &ast::Expr) -> bool {
    match &e.kind {
        ExprKind::Con(_) => true,
        ExprKind::App(f, _) | ExprKind::Paren(f) => con_headed(f),
        _ => false,
    }
}

/// The `Global` at the head of a reference applied to dictionaries.
fn head_global(e: &tast::Expr) -> &tast::Expr {
    match e {
        tast::Expr::App(f, _) => head_global(f),
        _ => e,
    }
}

/// Names referenced by an expression (over-approximated: locals have
/// unique names, so shadowing cannot confuse the analysis).
fn expr_free_vars(e: &ast::Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Var(q) => {
            out.insert(q.text());
        }
        ExprKind::Con(_) | ExprKind::Lit(_) | ExprKind::Error => {}
        ExprKind::App(f, a) => {
            expr_free_vars(f, out);
            expr_free_vars(a, out);
        }
        ExprKind::OpSeq(items) => {
            for i in items {
                if let ast::OpItem::Operand(x) = i {
                    expr_free_vars(x, out);
                }
                if let ast::OpItem::Op(q) = i {
                    out.insert(q.text());
                }
            }
        }
        ExprKind::Lam(_, b) | ExprKind::Paren(b) | ExprKind::Typed(b, _) => expr_free_vars(b, out),
        ExprKind::Let(ds, b) => {
            for d in ds {
                decl_free_vars(d, out);
            }
            expr_free_vars(b, out);
        }
        ExprKind::If(a, b, c) => {
            expr_free_vars(a, out);
            expr_free_vars(b, out);
            expr_free_vars(c, out);
        }
        ExprKind::MultiIf(gs) => {
            for g in gs {
                for s in &g.guards {
                    stmt_free_vars(s, out);
                }
                expr_free_vars(&g.body, out);
            }
        }
        ExprKind::Case(s, alts) => {
            expr_free_vars(s, out);
            for a in alts {
                rhs_free_vars(&a.rhs, out);
            }
        }
        ExprKind::LamCase(alts) => {
            for a in alts {
                rhs_free_vars(&a.rhs, out);
            }
        }
        ExprKind::Do(ss) => {
            for s in ss {
                stmt_free_vars(s, out);
            }
        }
        ExprKind::Tuple(es) | ExprKind::List(es) => {
            for x in es {
                expr_free_vars(x, out);
            }
        }
        ExprKind::TupleSection(es) => {
            for x in es.iter().flatten() {
                expr_free_vars(x, out);
            }
        }
        ExprKind::Range(a, b, c) => {
            expr_free_vars(a, out);
            if let Some(b) = b {
                expr_free_vars(b, out);
            }
            if let Some(c) = c {
                expr_free_vars(c, out);
            }
        }
        ExprKind::ListComp(h, qs) => {
            expr_free_vars(h, out);
            for s in qs {
                stmt_free_vars(s, out);
            }
        }
        ExprKind::LeftSection(x, op) | ExprKind::RightSection(op, x) => {
            out.insert(op.text());
            expr_free_vars(x, out);
        }
        ExprKind::RecordCon(_, fs) => {
            for (_, v) in fs {
                if let Some(v) = v {
                    expr_free_vars(v, out);
                }
            }
        }
        ExprKind::RecordUpd(x, fs) => {
            expr_free_vars(x, out);
            for (_, v) in fs {
                expr_free_vars(v, out);
            }
        }
    }
}

/// Strongly connected components in dependency order (Tarjan).
fn tarjan(n: usize, edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct S<'a> {
        edges: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    fn go(s: &mut S, v: usize) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on[v] = true;
        for &w in &s.edges[v] {
            match s.index[w] {
                None => {
                    go(s, w);
                    s.low[v] = s.low[v].min(s.low[w]);
                }
                Some(i) if s.on[w] => s.low[v] = s.low[v].min(i),
                _ => {}
            }
        }
        if Some(s.low[v]) == s.index[v] {
            let mut c = Vec::new();
            loop {
                let w = s.stack.pop().unwrap();
                s.on[w] = false;
                c.push(w);
                if w == v {
                    break;
                }
            }
            c.sort();
            s.out.push(c);
        }
    }
    let mut s = S {
        edges,
        index: vec![None; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if s.index[v].is_none() {
            go(&mut s, v);
        }
    }
    s.out
}

/// A pattern without its parentheses and type annotations.
fn strip_paren(p: &ast::Pat) -> &ast::Pat {
    match &p.kind {
        PatKind::Paren(q) | PatKind::Typed(q, _) => strip_paren(q),
        _ => p,
    }
}
