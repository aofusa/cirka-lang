//! The CirkaLang printer (08-ffi.md §6, §8): one `extern "C"` block with
//! the declarations `type`, `enum`, `const`, `struct`, `union` and the
//! functions. The syntax and meaning of the C aggregates in extern blocks
//! are those of FEAT-0250.

use crate::Options;
use crate::model::*;
use crate::print_hs::capitalized;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Reserved words of CirkaLang, the contextual keywords of extern blocks
/// and the ones planned for later versions.
const KEYWORDS: &[&str] = &[
    "module",
    "import",
    "data",
    "where",
    "let",
    "in",
    "case",
    "of",
    "if",
    "then",
    "else",
    "do",
    "private",
    "infix",
    "infixl",
    "infixr",
    "prim",
    "total",
    "partial",
    "covering",
    "trait",
    "impl",
    "mutual",
    "extern",
    "link",
    "type",
    "struct",
    "union",
    "enum",
    "const",
    "packed",
    "forall",
    "with",
    "rewrite",
    "requires",
    "ensures",
    "invariant",
    "trusted",
];

/// Names of the Prelude and `Foreign` in the name space of types and
/// constructors.
const TAKEN: &[&str] = &[
    "Int", "UInt", "Nat", "Float", "Char", "Bool", "String", "List", "Maybe", "Result", "IO",
    "IO1", "Unit", "Type", "LType", "Ptr", "OwnedPtr", "FunPtr", "CArray", "CVarArgs", "CChar",
    "CUChar", "CShort", "CUShort", "CInt", "CUInt", "CLong", "CULong", "CSize", "CSSize", "CFloat",
    "CDouble", "CBool", "Handle", "Array", "True", "False", "Nothing", "Just", "Ok", "Err", "Nil",
    "Cons", "MkUnit",
];

/// A lowercase identifier (`[a-z_][A-Za-z0-9_']*`, not `_`).
fn lower_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some('a'..='z' | '_')) && s != "_"
}

/// A field or parameter name: C names that are reserved words get a `_`
/// after them, names starting with an uppercase letter one before.
fn field_name(s: &str) -> String {
    if KEYWORDS.contains(&s) {
        format!("{s}_")
    } else if lower_ident(s) {
        s.to_string()
    } else {
        format!("_{s}")
    }
}

fn prim(p: Prim) -> &'static str {
    use Prim::*;
    match p {
        Bool => "CBool",
        Float => "CFloat",
        Double => "CDouble",
        Size => "CSize",
        SSize => "CSSize",
        _ => match (p.size(), p.signed()) {
            (1, true) => "CChar",
            (1, false) => "CUChar",
            (2, true) => "CShort",
            (2, false) => "CUShort",
            (4, true) => "CInt",
            (4, false) => "CUInt",
            (_, true) => "CLong",
            (_, false) => "CULong",
        },
    }
}

fn paren(atom: bool, s: String) -> String {
    if atom { format!("({s})") } else { s }
}

struct Printer<'a> {
    h: &'a Header,
    /// Output names of types, constants and enum constants (one name
    /// space), by key.
    names: HashMap<String, String>,
}

impl Printer<'_> {
    fn new(h: &Header) -> Printer<'_> {
        let mut taken: HashSet<String> = TAKEN.iter().map(|s| s.to_string()).collect();
        let mut names = HashMap::new();
        let mut fresh = |base: String| {
            let mut n = base;
            while !taken.insert(n.clone()) {
                n.push('_');
            }
            n
        };
        // Functions keep their C names (the symbols).
        for it in &h.items {
            if matches!(it.def, Def::Function(_)) {
                fresh(it.name.clone());
            }
        }
        for it in &h.items {
            match &it.def {
                Def::Function(_) | Def::Skipped(_) => {}
                _ => {
                    names.insert(it.key.clone(), fresh(capitalized(&it.name)));
                }
            }
            if let Def::Enum(e) = &it.def {
                for (c, _) in &e.consts {
                    names.insert(format!("{}/{c}", it.key), fresh(capitalized(c)));
                }
            }
        }
        Printer { h, names }
    }

    fn name(&self, key: &str) -> &str {
        self.names.get(key).map_or("()", String::as_str)
    }

    fn ty(&self, t: &Ty, atom: bool) -> String {
        match t {
            Ty::Void => "()".into(),
            Ty::Prim(p) => prim(*p).into(),
            Ty::Ptr(t) => paren(atom, format!("Ptr {}", self.ty(t, true))),
            Ty::FunPtr(t) => paren(atom, format!("FunPtr {}", self.ty(t, true))),
            Ty::Fn(s) => paren(atom, self.sig(s, &[], false)),
            Ty::Array(e, n) => paren(atom, format!("CArray {n} {}", self.ty(e, true))),
            Ty::Named(k) => self.name(k).to_string(),
            Ty::Unsupported(s) => format!("() {{- {s} -}}"),
        }
    }

    /// `(name : A) -> B -> ... -> IO R` (`R` alone when `pure`).
    fn sig(&self, s: &Sig, names: &[Option<String>], pure: bool) -> String {
        let mut out = String::new();
        for (i, p) in s.params.iter().enumerate() {
            match names.get(i).and_then(Option::as_ref) {
                Some(n) => write!(out, "({} : {}) -> ", field_name(n), self.ty(p, false)),
                None => write!(out, "{} -> ", self.ty(p, false)),
            }
            .ok();
        }
        if s.variadic {
            out += "... -> ";
        }
        if pure {
            out += &self.ty(&s.ret, false);
        } else {
            out += "IO ";
            out += &self.ty(&s.ret, true);
        }
        out
    }

    /// Why a C value of type `t` cannot cross the boundary.
    fn opaque_by_value(&self, t: &Ty) -> bool {
        match self.h.resolve(t) {
            Ty::Named(k) => matches!(self.h.get(k).map(|i| &i.def), Some(Def::Opaque(_))),
            _ => false,
        }
    }

    fn literal(&self, t: &Ty, v: &Value) -> String {
        match v {
            Value::Int(i) => i.to_string(),
            Value::Float(f) if self.h.prim(t) == Some(Prim::Float) => format!("{:?}", *f as f32),
            Value::Float(f) => format!("{f:?}"),
            Value::Str(s) => string(s),
        }
    }

    fn item(&self, o: &mut String, it: &Item, opts: &Options) {
        match &it.def {
            Def::Opaque(why) => {
                if let Some(w) = why {
                    writeln!(o, "  -- {}: opaque, {w}", it.name).ok();
                }
                writeln!(o, "  type {}", self.name(&it.key)).ok();
            }
            Def::Record(r) => {
                let kw = if r.union { "union" } else { "struct" };
                let packed = if r.packed { "packed " } else { "" };
                for n in &r.notes {
                    writeln!(o, "  -- {}: {n}", it.name).ok();
                }
                writeln!(o, "  {packed}{kw} {} where", self.name(&it.key)).ok();
                for f in &r.fields {
                    write!(o, "    {} : {}", field_name(&f.name), self.ty(&f.ty, false)).ok();
                    if !f.bits.is_empty() {
                        let bits: Vec<String> = f
                            .bits
                            .iter()
                            .map(|b| {
                                format!("{}: bits {}..{}", b.name, b.offset, b.offset + b.width)
                            })
                            .collect();
                        write!(o, "  -- {}", bits.join(", ")).ok();
                    }
                    o.push('\n');
                }
            }
            Def::Enum(e) => {
                writeln!(o, "  enum {} : {} where", self.name(&it.key), prim(e.int)).ok();
                for (c, v) in &e.consts {
                    writeln!(o, "    {} = {v}", self.name(&format!("{}/{c}", it.key))).ok();
                }
            }
            Def::Alias(t) => {
                writeln!(o, "  type {} = {}", self.name(&it.key), self.ty(t, false)).ok();
            }
            Def::Const(c) => {
                let ty = match c.value {
                    Value::Str(_) => "String".to_string(),
                    _ => self.ty(&c.ty, false),
                };
                writeln!(
                    o,
                    "  const {} : {ty} = {}",
                    self.name(&it.key),
                    self.literal(&c.ty, &c.value)
                )
                .ok();
            }
            Def::Function(f) => {
                let s = &f.sig;
                if !lower_ident(&it.name) || KEYWORDS.contains(&it.name.as_str()) {
                    writeln!(o, "  -- skipped {}: not a CirkaLang value name", it.name).ok();
                } else if s
                    .params
                    .iter()
                    .chain([&s.ret])
                    .any(|t| self.opaque_by_value(t))
                {
                    writeln!(
                        o,
                        "  -- skipped {}: passes an opaque type by value",
                        it.name
                    )
                    .ok();
                } else {
                    let pure = s.ret != Ty::Void
                        && opts.pure.as_ref().is_some_and(|p| p.matches(&it.name));
                    writeln!(o, "  {} : {}", it.name, self.sig(s, &f.names, pure)).ok();
                }
            }
            Def::Skipped(why) => {
                writeln!(o, "  -- skipped {}: {why}", it.name).ok();
            }
        }
    }
}

/// A CirkaLang string literal.
fn string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            ' '..='~' => out.push(c),
            _ => {
                write!(out, "\\u{{{:x}}}", c as u32).ok();
            }
        }
    }
    out.push('"');
    out
}

/// The module for the declarations of `h`.
pub fn print(h: &Header, opts: &Options, source: &str) -> String {
    let p = Printer::new(h);
    let mut o = String::new();
    writeln!(o, "-- Generated by `cirka bindgen` from {source}.").ok();
    writeln!(
        o,
        "-- Pointers are raw `Ptr`s: wrap the functions in safe types."
    )
    .ok();
    if let Some(m) = &opts.module {
        writeln!(o, "module {m}").ok();
    }
    writeln!(o, "\nimport Foreign\n").ok();
    o.push_str("extern \"C\"");
    for l in &opts.links {
        write!(o, " link {}", string(l)).ok();
    }
    o.push_str(" where\n");
    for (i, it) in h.items.iter().enumerate() {
        if i > 0 {
            o.push('\n');
        }
        p.item(&mut o, it, opts);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_types() {
        assert_eq!(field_name("type"), "type_");
        assert_eq!(field_name("X"), "_X");
        assert_eq!(field_name("sType"), "sType");
        assert_eq!(prim(Prim::U32), "CUInt");
        assert_eq!(prim(Prim::LongLong), "CLong");
        assert_eq!(prim(Prim::I8), "CChar");
        assert_eq!(string("a\u{3042}\n"), "\"a\\u{3042}\\n\"");
    }
}
