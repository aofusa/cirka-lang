//! Top-level entities, exports and import scopes.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ns {
    /// Variables, constructors, fields and methods.
    Value,
    /// Types and classes.
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Var,
    Con,
    Field,
    Method,
    Type,
    Class,
}

impl Kind {
    pub fn ns(self) -> Ns {
        match self {
            Kind::Type | Kind::Class => Ns::Type,
            _ => Ns::Value,
        }
    }
}

/// A top-level entity: its defining module and name, and the type or
/// class it belongs to (constructors, fields, methods).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ent {
    pub kind: Kind,
    pub module: String,
    pub name: String,
    pub parent: Option<String>,
}

/// Names in scope: `(namespace, qualifier, name)` to entities.
#[derive(Default, Clone)]
pub struct Scope {
    pub map: HashMap<(Ns, Option<String>, String), Vec<Ent>>,
}

impl Scope {
    pub fn add(&mut self, qual: Option<String>, e: Ent) {
        let v = self
            .map
            .entry((e.kind.ns(), qual, e.name.clone()))
            .or_default();
        if !v.contains(&e) {
            v.push(e);
        }
    }

    pub fn lookup(&self, ns: Ns, qual: Option<&str>, name: &str) -> &[Ent] {
        self.map
            .get(&(ns, qual.map(str::to_string), name.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Entities in scope both unqualified and with qualifier `m`.
    pub fn via_module(&self, m: &str) -> Vec<Ent> {
        let mut out = Vec::new();
        for ((ns, q, n), es) in &self.map {
            if q.as_deref() != Some(m) {
                continue;
            }
            let unq = self.lookup(*ns, None, n);
            for e in es {
                if unq.contains(e) && !out.contains(e) {
                    out.push(e.clone());
                }
            }
        }
        out.sort_by(|a, b| (&a.module, &a.name).cmp(&(&b.module, &b.name)));
        out
    }
}

/// What a module exports.
#[derive(Default, Clone, Debug)]
pub struct Exports {
    pub ents: Vec<Ent>,
}

impl Exports {
    pub fn children(&self, parent: &Ent) -> Vec<Ent> {
        self.ents
            .iter()
            .filter(|e| e.parent.as_deref() == Some(&parent.name) && e.module == parent.module)
            .cloned()
            .collect()
    }
}
