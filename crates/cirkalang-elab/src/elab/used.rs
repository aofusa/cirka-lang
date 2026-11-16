//! The items of extern blocks that a program uses (FEAT-0255,
//! spec-v0.0.6.md §1.10).
//!
//! The extern block of a large C header expands to tens of thousands of
//! definitions and implementations (the field functions, `Storable`, `Eq`
//! and `Show` of every struct), of which a program uses few. The items
//! written in the source are used; so is every global that a used item
//! names, and an implementation of an extern block when the types of its
//! head are used. The items expanded from structs and unions
//! ([`Origin::ExternLazy`]) are well-typed when their declarations are
//! (§1.9): their bodies are elaborated only when used. The other items of
//! extern blocks ([`Origin::Extern`]) and the extern functions are always
//! checked, since their errors are the declarations'. Items of extern
//! blocks that are not used are left out of the program.

use cirkalang_resolve::rast::{self, GName, Origin};
use std::collections::{HashMap, HashSet};

/// The items of extern blocks and what each of them uses.
pub(super) struct Uses<'a> {
    defs: HashMap<GName, &'a rast::Def>,
    datas: HashMap<GName, &'a rast::Data>,
    /// The data type of each constructor of `datas`.
    con_data: HashMap<GName, GName>,
    externs: HashMap<GName, &'a rast::ExternFn>,
    /// Implementations with the types of their heads.
    impls: Vec<(&'a rast::Impl, Vec<GName>)>,
    /// The positions in `impls` by the types of their heads.
    impls_by_type: HashMap<GName, Vec<usize>>,
    /// The implementations by name.
    impl_pos: HashMap<GName, usize>,
}

impl<'a> Uses<'a> {
    pub(super) fn new(items: &[&'a rast::Item]) -> Uses<'a> {
        let mut u = Uses {
            defs: HashMap::new(),
            datas: HashMap::new(),
            con_data: HashMap::new(),
            externs: HashMap::new(),
            impls: Vec::new(),
            impls_by_type: HashMap::new(),
            impl_pos: HashMap::new(),
        };
        for it in items {
            match it {
                rast::Item::Def(d) if d.origin != Origin::Source => {
                    u.defs.insert(d.name, d);
                }
                rast::Item::Data(d) if d.origin != Origin::Source => {
                    for c in &d.cons {
                        u.con_data.insert(c.name, d.name);
                    }
                    u.datas.insert(d.name, d);
                }
                rast::Item::Extern(f) => {
                    u.externs.insert(f.name, f);
                }
                rast::Item::Impl(i) if i.origin != Origin::Source => {
                    let mut heads = Vec::new();
                    rast::globals_in_expr(&i.head, &mut |g| {
                        if g != i.trait_ && !heads.contains(&g) {
                            heads.push(g);
                        }
                    });
                    let k = u.impls.len();
                    for g in &heads {
                        u.impls_by_type.entry(*g).or_default().push(k);
                    }
                    u.impl_pos.insert(i.name, k);
                    u.impls.push((i, heads));
                }
                _ => {}
            }
        }
        u
    }

    /// The globals that the items written in the source use.
    pub(super) fn of_source(&self, items: &[&rast::Item]) -> HashSet<GName> {
        let mut roots = Vec::new();
        let mut f = |g: GName| roots.push(g);
        for it in items {
            match it {
                rast::Item::Def(d) if d.origin == Origin::Source => {
                    f(d.name);
                    def_uses(d, &mut f);
                }
                rast::Item::Data(d) if d.origin == Origin::Source => data_uses(d, &mut f),
                rast::Item::Impl(i) if i.origin == Origin::Source => impl_uses(i, &mut f),
                // Not for a type: used.
                rast::Item::Impl(i)
                    if self
                        .impl_pos
                        .get(&i.name)
                        .is_some_and(|&k| self.impls[k].1.is_empty()) =>
                {
                    f(i.name)
                }
                rast::Item::Prim(_, t) => rast::globals_in_expr(t, &mut f),
                rast::Item::Trait(t) => {
                    for m in &t.methods {
                        if let Some((g, cs)) = &m.default {
                            f(*g);
                            rast::globals_in_clauses(cs, &mut f);
                        }
                    }
                }
                _ => {}
            }
        }
        let mut used = HashSet::new();
        self.add(&mut used, roots);
        used
    }

    /// Adds `gs` to `used`, with everything they use; returns the globals
    /// that were not used before.
    pub(super) fn add(&self, used: &mut HashSet<GName>, gs: Vec<GName>) -> Vec<GName> {
        let mut new = Vec::new();
        let mut stack = gs;
        while let Some(g) = stack.pop() {
            if !used.insert(g) {
                continue;
            }
            new.push(g);
            let mut f = |x: GName| stack.push(x);
            if let Some(d) = self.defs.get(&g) {
                def_uses(d, &mut f);
            } else if let Some(d) = self.datas.get(&g) {
                data_uses(d, &mut f);
            } else if let Some(&d) = self.con_data.get(&g) {
                f(d);
            } else if let Some(x) = self.externs.get(&g) {
                rast::globals_in_expr(&x.ty, &mut f);
            } else if let Some(&k) = self.impl_pos.get(&g) {
                impl_uses(self.impls[k].0, &mut f);
            }
            // The implementations for a type are used with it.
            for &k in self.impls_by_type.get(&g).into_iter().flatten() {
                let (i, heads) = &self.impls[k];
                if !used.contains(&i.name) && heads.iter().all(|h| *h == g || used.contains(h)) {
                    stack.push(i.name);
                }
            }
        }
        new
    }

    /// The items of extern blocks that are not in `used`: definitions,
    /// data types, functions, and implementations with the globals of
    /// their methods.
    pub(super) fn unused(&self, used: &HashSet<GName>) -> HashSet<GName> {
        let mut out: HashSet<GName> = self
            .defs
            .keys()
            .chain(self.datas.keys())
            .chain(self.externs.keys())
            .filter(|g| !used.contains(g))
            .copied()
            .collect();
        for (i, _) in &self.impls {
            if !used.contains(&i.name) {
                out.insert(i.name);
                out.extend(i.methods.iter().map(|m| m.def));
            }
        }
        out
    }
}

/// An item of [`Uses`].
pub(super) enum Item<'a> {
    Def(&'a rast::Def),
    Impl(&'a rast::Impl),
}

impl<'a> Uses<'a> {
    /// The definition or implementation of an extern block named `g`.
    pub(super) fn item(&self, g: GName) -> Option<Item<'a>> {
        if let Some(d) = self.defs.get(&g) {
            return Some(Item::Def(d));
        }
        self.impl_pos.get(&g).map(|&k| Item::Impl(self.impls[k].0))
    }
}

fn def_uses(d: &rast::Def, f: &mut dyn FnMut(GName)) {
    rast::globals_in_expr(&d.sig, f);
    rast::globals_in_clauses(&d.clauses, f);
}

fn data_uses(d: &rast::Data, f: &mut dyn FnMut(GName)) {
    for p in &d.params {
        if let Some(t) = &p.ty {
            rast::globals_in_expr(t, f);
        }
    }
    if let Some(s) = &d.sig {
        rast::globals_in_expr(s, f);
    }
    for c in &d.cons {
        match &c.body {
            rast::ConBody::Sig(t) => rast::globals_in_expr(t, f),
            rast::ConBody::Fields(ts) => {
                for t in ts {
                    rast::globals_in_expr(t, f);
                }
            }
        }
    }
}

fn impl_uses(i: &rast::Impl, f: &mut dyn FnMut(GName)) {
    for e in i.kinds.iter().chain(&i.context) {
        rast::globals_in_expr(e, f);
    }
    rast::globals_in_expr(&i.head, f);
    for m in &i.methods {
        f(m.def);
        rast::globals_in_clauses(&m.clauses, f);
    }
}
