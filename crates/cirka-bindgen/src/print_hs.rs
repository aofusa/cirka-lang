//! The aCHC printer (08-ffi.md §6): Haskell 2010 FFI imports and records
//! with `Storable` instances that read and write the fields at clang's
//! offsets. The output is plain Haskell, usable from `.hs` (also with GHC)
//! and `.chs`.

use crate::Options;
use crate::model::*;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Names taken by the Prelude and the imported modules in the type and
/// constructor namespace.
const TAKEN_TYPES: &[&str] = &[
    "Bool",
    "Char",
    "Double",
    "Float",
    "Int",
    "Integer",
    "Word",
    "IO",
    "Maybe",
    "Either",
    "Ordering",
    "String",
    "Ptr",
    "FunPtr",
    "Storable",
    "Eq",
    "Ord",
    "Show",
    "Read",
    "Num",
    "Enum",
    "Bounded",
    "Integral",
    "Real",
    "Fractional",
    "Floating",
    "Functor",
    "Applicative",
    "Monad",
    "Int8",
    "Int16",
    "Int32",
    "Int64",
    "Word8",
    "Word16",
    "Word32",
    "Word64",
    "CChar",
    "CSChar",
    "CUChar",
    "CShort",
    "CUShort",
    "CInt",
    "CUInt",
    "CLong",
    "CULong",
    "CLLong",
    "CULLong",
    "CPtrdiff",
    "CSize",
    "CWchar",
    "CIntPtr",
    "CUIntPtr",
    "CIntMax",
    "CUIntMax",
    "CBool",
    "CTime",
    "CFloat",
    "CDouble",
    "IntPtr",
    "WordPtr",
    "FilePath",
    "IOError",
    "True",
    "False",
    "Nothing",
    "Just",
    "Left",
    "Right",
    "LT",
    "EQ",
    "GT",
];

struct Names {
    types: HashMap<String, String>,
    values: HashMap<String, String>,
    taken_types: HashSet<String>,
    taken_values: HashSet<String>,
}

fn fresh(taken: &mut HashSet<String>, base: String) -> String {
    let mut n = base;
    while !taken.insert(n.clone()) {
        n.push('\'');
    }
    n
}

/// `name` as a Haskell type name (without leading underscores, the first
/// letter capitalized).
pub(crate) fn capitalized(name: &str) -> String {
    let s = name.trim_start_matches('_');
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => "Anon".to_string(),
    }
}

fn lower_first(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_ascii_lowercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

impl Names {
    fn new(h: &Header) -> Names {
        let mut n = Names {
            types: HashMap::new(),
            values: HashMap::new(),
            taken_types: TAKEN_TYPES.iter().map(|s| s.to_string()).collect(),
            taken_values: HashSet::new(),
        };
        for it in &h.items {
            match &it.def {
                Def::Opaque(_) | Def::Record(_) | Def::Enum(_) | Def::Alias(_) => {
                    let t = fresh(&mut n.taken_types, capitalized(&it.name));
                    n.types.insert(it.key.clone(), t);
                }
                Def::Const(_) | Def::Function(_) => {
                    let v = fresh(&mut n.taken_values, format!("c_{}", it.name));
                    n.values.insert(it.key.clone(), v);
                }
                Def::Skipped(_) => {}
            }
            if let Def::Enum(e) = &it.def {
                for (c, _) in &e.consts {
                    let v = fresh(&mut n.taken_values, format!("c_{c}"));
                    n.values.insert(format!("{}/{c}", it.key), v);
                }
            }
        }
        // Record fields: `<type with a lowercase first letter>_<field>`.
        for it in &h.items {
            if let Def::Record(r) = &it.def
                && !r.union
            {
                let prefix = lower_first(&n.types[&it.key]);
                for (i, f) in r.fields.iter().enumerate() {
                    let v = fresh(&mut n.taken_values, format!("{prefix}_{}", f.name));
                    n.values.insert(format!("{}#{i}", it.key), v);
                }
            }
        }
        n
    }

    fn ty(&self, key: &str) -> &str {
        self.types.get(key).map_or("()", String::as_str)
    }
}

fn prim(p: Prim) -> &'static str {
    use Prim::*;
    match p {
        Bool => "CBool",
        Char => "CChar",
        SChar => "CSChar",
        UChar => "CUChar",
        Short => "CShort",
        UShort => "CUShort",
        Int => "CInt",
        UInt => "CUInt",
        Long | SSize => "CLong",
        ULong => "CULong",
        LongLong => "CLLong",
        ULongLong => "CULLong",
        Float => "CFloat",
        Double => "CDouble",
        WChar => "CWchar",
        I8 => "Int8",
        U8 => "Word8",
        I16 => "Int16",
        U16 => "Word16",
        I32 => "Int32",
        U32 => "Word32",
        I64 => "Int64",
        U64 => "Word64",
        Size => "CSize",
        IntPtr => "CIntPtr",
        UIntPtr => "CUIntPtr",
        PtrDiff => "CPtrdiff",
        IntMax => "CIntMax",
        UIntMax => "CUIntMax",
    }
}

fn paren(atom: bool, s: String) -> String {
    if atom { format!("({s})") } else { s }
}

/// The element type and the number of elements of a (multi-dimensional)
/// array, which Haskell holds as one flat list.
fn flat(t: &Ty) -> (&Ty, u64) {
    match t {
        Ty::Array(e, n) => {
            let (e, m) = flat(e);
            (e, n * m)
        }
        _ => (t, 1),
    }
}

struct Printer<'a> {
    h: &'a Header,
    n: Names,
    out: String,
}

impl Printer<'_> {
    fn ty(&self, t: &Ty, atom: bool) -> String {
        match t {
            Ty::Void => "()".into(),
            Ty::Prim(p) => prim(*p).into(),
            Ty::Ptr(t) => paren(atom, format!("Ptr {}", self.ty(t, true))),
            Ty::FunPtr(t) => paren(atom, format!("FunPtr {}", self.ty(t, true))),
            Ty::Fn(s) => paren(atom, self.sig(s, false)),
            Ty::Array(..) => format!("[{}]", self.ty(flat(t).0, false)),
            Ty::Named(k) => self.n.ty(k).to_string(),
            Ty::Unsupported(s) => format!("() {{- {s} -}}"),
        }
    }

    /// `A -> B -> IO R` (`R` alone when `pure`).
    fn sig(&self, s: &Sig, pure: bool) -> String {
        let mut out = String::new();
        for p in &s.params {
            out += &self.ty(p, false);
            out += " -> ";
        }
        if pure {
            out += &self.ty(&s.ret, false);
        } else {
            out += "IO ";
            out += &self.ty(&s.ret, true);
        }
        out
    }

    /// Why the Haskell 2010 FFI cannot call a function of this type.
    fn not_callable(&self, s: &Sig) -> Option<String> {
        if s.variadic {
            return Some("a variadic function (not in the Haskell 2010 FFI)".into());
        }
        s.params
            .iter()
            .chain(std::iter::once(&s.ret))
            .any(|t| self.h.is_record(t))
            .then(|| "passes a struct or union by value (not in the Haskell 2010 FFI)".into())
    }

    fn literal(&self, t: &Ty, v: &Value) -> String {
        let float32 = self.h.prim(t) == Some(Prim::Float);
        let lit = match v {
            Value::Int(i) => i.to_string(),
            Value::Float(f) if float32 => format!("{:?}", *f as f32),
            Value::Float(f) => format!("{f:?}"),
            Value::Str(s) => return string(s),
        };
        match self.h.resolve(t) {
            Ty::Named(k) if matches!(self.h.get(k).map(|i| &i.def), Some(Def::Enum(_))) => {
                let lit = if lit.starts_with('-') {
                    format!("({lit})")
                } else {
                    lit
                };
                format!("{} {lit}", self.n.ty(k))
            }
            _ => lit,
        }
    }

    fn item(&mut self, it: &Item, opts: &Options) {
        let mut o = String::new();
        match &it.def {
            Def::Opaque(why) => {
                match why {
                    Some(w) => writeln!(o, "-- | {} (opaque: {w})", it.name),
                    None => writeln!(o, "-- | {} (opaque)", it.name),
                }
                .ok();
                writeln!(o, "data {}", self.n.ty(&it.key)).ok();
            }
            Def::Record(r) if r.union => self.union(&mut o, it, r),
            Def::Record(r) => self.record(&mut o, it, r),
            Def::Enum(e) => self.enumeration(&mut o, it, e),
            Def::Alias(t) => self.alias(&mut o, it, t),
            Def::Const(c) => {
                let v = &self.n.values[&it.key];
                let ty = match c.value {
                    Value::Str(_) => "String".to_string(),
                    _ => self.ty(&c.ty, false),
                };
                writeln!(o, "{v} :: {ty}").ok();
                writeln!(o, "{v} = {}", self.literal(&c.ty, &c.value)).ok();
            }
            Def::Function(f) => {
                if let Some(why) = self.not_callable(&f.sig) {
                    writeln!(o, "-- skipped {}: {why}", it.name).ok();
                } else {
                    let pure = f.sig.ret != Ty::Void
                        && opts.pure.as_ref().is_some_and(|p| p.matches(&it.name));
                    writeln!(
                        o,
                        "foreign import ccall \"{}\" {} :: {}",
                        it.name,
                        self.n.values[&it.key],
                        self.sig(&f.sig, pure)
                    )
                    .ok();
                }
            }
            Def::Skipped(why) => {
                writeln!(o, "-- skipped {}: {why}", it.name).ok();
            }
        }
        self.out.push('\n');
        self.out.push_str(&o);
    }

    fn record(&self, o: &mut String, it: &Item, r: &Record) {
        let t = self.n.ty(&it.key);
        writeln!(o, "-- | struct {}", it.name).ok();
        for note in &r.notes {
            writeln!(o, "-- ({note})").ok();
        }
        writeln!(o, "data {t} = {t}").ok();
        for (i, f) in r.fields.iter().enumerate() {
            let sep = if i == 0 { '{' } else { ',' };
            let field = &self.n.values[&format!("{}#{i}", it.key)];
            write!(o, "  {sep} {field} :: {}", self.ty(&f.ty, false)).ok();
            if !f.bits.is_empty() {
                let bits: Vec<String> = f
                    .bits
                    .iter()
                    .map(|b| format!("{}: bits {}..{}", b.name, b.offset, b.offset + b.width))
                    .collect();
                write!(o, " -- {}", bits.join(", ")).ok();
            }
            o.push('\n');
        }
        writeln!(o, "  }} deriving (Eq, Show)\n").ok();
        let vars: Vec<String> = (0..r.fields.len()).map(|i| format!("v{i}")).collect();
        writeln!(o, "instance Storable {t} where").ok();
        writeln!(o, "  sizeOf _ = {}", r.size).ok();
        writeln!(o, "  alignment _ = {}", r.align).ok();
        writeln!(o, "  peek p = do").ok();
        for (v, f) in vars.iter().zip(&r.fields) {
            match flat(&f.ty) {
                (_, n) if matches!(f.ty, Ty::Array(..)) => {
                    writeln!(o, "    {v} <- peekArray {n} (plusPtr p {})", f.offset).ok()
                }
                _ => writeln!(o, "    {v} <- peekByteOff p {}", f.offset).ok(),
            };
        }
        writeln!(o, "    return ({t} {})", vars.join(" ")).ok();
        writeln!(o, "  poke p ({t} {}) = do", vars.join(" ")).ok();
        for (v, f) in vars.iter().zip(&r.fields) {
            match flat(&f.ty) {
                (_, n) if matches!(f.ty, Ty::Array(..)) => {
                    writeln!(o, "    pokeArray (plusPtr p {}) (take {n} {v})", f.offset).ok()
                }
                _ => writeln!(o, "    pokeByteOff p {} {v}", f.offset).ok(),
            };
        }
    }

    fn union(&self, o: &mut String, it: &Item, r: &Record) {
        let t = self.n.ty(&it.key);
        let members: Vec<String> = r
            .fields
            .iter()
            .map(|f| format!("{} :: {}", f.name, self.ty(&f.ty, false)))
            .collect();
        writeln!(o, "-- | union {}: {}", it.name, members.join(", ")).ok();
        writeln!(
            o,
            "-- (the bytes of the union; read a member through a pointer)"
        )
        .ok();
        writeln!(o, "newtype {t} = {t} [Word8] deriving (Eq, Show)\n").ok();
        writeln!(o, "instance Storable {t} where").ok();
        writeln!(o, "  sizeOf _ = {}", r.size).ok();
        writeln!(o, "  alignment _ = {}", r.align).ok();
        writeln!(o, "  peek p = fmap {t} (peekArray {} (castPtr p))", r.size).ok();
        writeln!(
            o,
            "  poke p ({t} v) = pokeArray (castPtr p) (take {} v)",
            r.size
        )
        .ok();
    }

    fn enumeration(&self, o: &mut String, it: &Item, e: &Enum) {
        let t = self.n.ty(&it.key);
        let size = e.int.size();
        writeln!(o, "-- | enum {}", it.name).ok();
        writeln!(
            o,
            "newtype {t} = {t} {} deriving (Eq, Ord, Show)\n",
            prim(e.int)
        )
        .ok();
        writeln!(o, "instance Storable {t} where").ok();
        writeln!(o, "  sizeOf _ = {size}").ok();
        writeln!(o, "  alignment _ = {size}").ok();
        writeln!(o, "  peek p = fmap {t} (peek (castPtr p))").ok();
        writeln!(o, "  poke p ({t} v) = poke (castPtr p) v").ok();
        for (c, v) in &e.consts {
            let name = &self.n.values[&format!("{}/{c}", it.key)];
            let v = if *v < 0 {
                format!("({v})")
            } else {
                v.to_string()
            };
            writeln!(o, "\n{name} :: {t}\n{name} = {t} {v}").ok();
        }
    }

    fn alias(&self, o: &mut String, it: &Item, t: &Ty) {
        let name = self.n.ty(&it.key);
        writeln!(o, "type {name} = {}", self.ty(t, false)).ok();
        // Calling a function pointer of this type, and making one from a
        // Haskell function.
        if let Ty::FunPtr(f) = t
            && let Some(s) = self.h.sig(f)
            && self.not_callable(s).is_none()
        {
            let sig = self.sig(s, false);
            writeln!(
                o,
                "\nforeign import ccall \"dynamic\" dyn_{name} :: {name} -> {sig}"
            )
            .ok();
            writeln!(
                o,
                "foreign import ccall \"wrapper\" wrap_{name} :: ({sig}) -> IO {name}"
            )
            .ok();
        }
    }
}

/// A Haskell string literal.
fn string(s: &str) -> String {
    let mut out = String::from("\"");
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            _ => {
                write!(out, "\\{}", c as u32).ok();
                if chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                    out.push_str("\\&");
                }
            }
        }
    }
    out.push('"');
    out
}

/// The module for the declarations of `h`.
pub fn print(h: &Header, opts: &Options, source: &str) -> String {
    let mut p = Printer {
        h,
        n: Names::new(h),
        out: String::new(),
    };
    let module = opts.module.as_deref().unwrap_or("Bindings");
    let mut o = String::new();
    writeln!(o, "-- Generated by `achc bindgen` from {source}.").ok();
    writeln!(
        o,
        "-- Pointers are raw `Ptr`s: wrap the functions in safe types."
    )
    .ok();
    for l in &opts.links {
        writeln!(o, "-- Link with -l{l}.").ok();
    }
    writeln!(o, "module {module} where\n").ok();
    for m in [
        "Data.Int",
        "Data.Word",
        "Foreign.C.Types",
        "Foreign.Marshal.Array (peekArray, pokeArray)",
        "Foreign.Ptr",
        "Foreign.Storable",
    ] {
        writeln!(o, "import {m}").ok();
    }
    p.out = o;
    for it in &h.items {
        p.item(it, opts);
    }
    p.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_literals() {
        assert_eq!(capitalized("timespec"), "Timespec");
        assert_eq!(capitalized("_IO_FILE"), "IO_FILE");
        assert_eq!(lower_first("VkExtent2D"), "vkExtent2D");
        assert_eq!(string("a\"\n\u{3042}1"), "\"a\\\"\\n\\12354\\&1\"");
        assert_eq!(
            flat(&Ty::Array(Box::new(Ty::Array(Box::new(Ty::Void), 4)), 3)).1,
            12
        );
    }
}
