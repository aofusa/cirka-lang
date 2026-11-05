//! achc-typeck: Hindley–Milner type inference for aCHC (H1).
//!
//! The renamed program is checked as a whole: data types and synonyms of
//! all modules are declared first, then the top-level bindings are split
//! into strongly connected components and inferred in dependency order
//! with let-polymorphism. Bindings with a signature are checked against it
//! (and may be polymorphically recursive). The output is a typed program
//! (`tast`) in which syntactic sugar is gone except pattern matching.
//! Type classes (FEAT-0110) use dictionary passing (`classes`); derived
//! instances are generated as instance declarations (`deriving`).

mod classes;
mod deriving;
pub mod infer;
mod linear;
mod raii;
pub mod tast;
pub mod types;

use achc_syntax::ast::Decl;
use cirka_diagnostics::{Diagnostic, Span};
use infer::Tc;
use types::*;

/// Type checks a renamed program. On success also returns warnings.
pub fn typecheck(p: &achc_rename::Program) -> Result<tast::Program, Vec<Diagnostic>> {
    typecheck_in(p, None, &|_| false)
}

/// Like [`typecheck`]; with the source map, calls of `error` and
/// `undefined` in modules outside the library (`is_library` tells library
/// files by name) report their call site like GHC's `HasCallStack`.
pub fn typecheck_in(
    p: &achc_rename::Program,
    sm: Option<&cirka_diagnostics::SourceMap>,
    is_library: &dyn Fn(&str) -> bool,
) -> Result<tast::Program, Vec<Diagnostic>> {
    let mut tc = Tc::new();
    if let Some(sm) = sm {
        let mods = p
            .modules
            .iter()
            .filter(|m| sm.get(m.file).is_some_and(|f| !is_library(&f.name)))
            .map(|m| (m.file.0, m.name.clone()))
            .collect();
        tc.sources = Some((sm.clone(), mods));
    }
    let modules: Vec<(String, Vec<Decl>)> = p
        .modules
        .iter()
        .map(|m| (m.name.clone(), m.ast.decls.clone()))
        .collect();
    tc.has_ltype = modules.iter().any(|(_, ds)| ds.iter().any(mentions_ltype));
    tc.declare_types(&modules);
    let mut binds = tc.field_selectors();
    let (mut values, class_binds) = tc.declare_classes(&modules);
    binds.extend(class_binds);
    // Derived instances join the declared ones.
    let mut with_derived = modules.clone();
    for (m, d) in tc.derive_instances(&modules) {
        if let Some(x) = with_derived.iter_mut().find(|x| x.0 == m) {
            x.1.push(d);
        }
    }
    let (inst_values, mut dfuns) = tc.declare_instances(&with_derived);
    dfuns.extend(tc.declare_newtype_instances());
    values.extend(inst_values);
    for (m, ds) in &modules {
        for d in ds {
            if matches!(d, Decl::Fun(..) | Decl::PatBind(..) | Decl::TypeSig(..)) {
                values.push((m.clone(), d.clone()));
            }
        }
    }
    binds.extend(tc.bind_group(&values, true));
    binds.extend(dfuns);
    // `main :: IO t` in the main module.
    let main = p.modules.last().and_then(|m| {
        let name = format!("{}.main", m.name);
        let s = tc.globals.get(&name)?.clone();
        let (t, _) = tc.instantiate(&s);
        let r = tc.fresh();
        let io = Ty::App(Box::new(Ty::Con(prim("IO"))), Box::new(r));
        let span = main_span(&m.ast.decls).unwrap_or(m.ast.span);
        tc.unify(span, &io, &t);
        Some(name)
    });
    tc.solve_top();
    if tc.errors.is_empty() {
        let files = p.modules.iter().map(|m| (m.name.clone(), m.file)).collect();
        let (errs, plan) = linear::check(&mut tc, &modules, &files);
        tc.errors.extend(errs);
        if tc.errors.is_empty() && tc.has_ltype {
            raii::apply(&mut tc, &plan, &mut binds);
            // Strict-Linear (§6): fields of linear-only types are strict.
            let mut datas = std::mem::take(&mut tc.out.datas);
            for d in datas.iter_mut() {
                for c in d.cons.iter_mut() {
                    for (f, s) in c.fields.iter().zip(c.strict.iter_mut()) {
                        if tc.is_ltype(f) {
                            *s = true;
                        }
                    }
                }
            }
            tc.out.datas = datas;
        }
    }
    for g in binds.iter_mut() {
        tc.finish_group(g);
    }
    if !tc.errors.is_empty() {
        let mut errs = std::mem::take(&mut tc.errors);
        errs.sort_by_key(|d| d.primary_span().map(|s| (s.file.0, s.start)));
        return Err(errs);
    }
    let mut out = std::mem::take(&mut tc.out);
    out.binds = binds;
    out.main = main;
    Ok(out)
}

/// Whether a declaration uses the kind `LType` (then uniqueness is
/// checked, FEAT-0121).
fn mentions_ltype(d: &Decl) -> bool {
    use achc_syntax::ast::{KindSig, Type, TypeKind};
    fn kind(k: &KindSig) -> bool {
        match k {
            KindSig::LType => true,
            KindSig::Star => false,
            KindSig::Arrow(a, b) => kind(a) || kind(b),
        }
    }
    fn ty(t: &Type) -> bool {
        match &t.kind {
            TypeKind::Forall(vs, b) => {
                vs.iter().any(|(_, k)| k.as_ref().is_some_and(kind)) || ty(b)
            }
            TypeKind::Kinded(b, k) => kind(k) || ty(b),
            TypeKind::Qual(_, b) => ty(b),
            _ => false,
        }
    }
    match d {
        Decl::KindSig(_, k, _) => kind(k),
        Decl::TypeSig(_, t, _) => ty(t),
        Decl::Class(c) => {
            c.param_kinds.iter().flatten().any(kind) || c.decls.iter().any(mentions_ltype)
        }
        Decl::Instance(i) => i.decls.iter().any(mentions_ltype),
        _ => false,
    }
}

fn main_span(ds: &[Decl]) -> Option<Span> {
    ds.iter().find_map(|d| match d {
        Decl::Fun(f, _, _, sp) if f.name == "main" => Some(*sp),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
