//! The declarations of a header, independent of the output language
//! (08-ffi.md §6). Records and enums are identified by keys (clang's USRs,
//! or a name made from the parent for anonymous ones), so that a type can
//! refer to a declaration that comes later.

use cirka_core::CTy;
use std::collections::HashMap;

/// A C scalar type. The `stdint.h` / `stddef.h` names are kept apart from
/// the builtin types they are typedefs of, so that printers can use the
/// language's names for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    Bool,
    Char,
    SChar,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    LongLong,
    ULongLong,
    Float,
    Double,
    WChar,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    Size,
    SSize,
    IntPtr,
    UIntPtr,
    PtrDiff,
    IntMax,
    UIntMax,
}

impl Prim {
    /// Size in bytes on x86_64 Linux (also the alignment).
    pub fn size(self) -> u64 {
        use Prim::*;
        match self {
            Bool | Char | SChar | UChar | I8 | U8 => 1,
            Short | UShort | I16 | U16 => 2,
            Int | UInt | WChar | I32 | U32 | Float => 4,
            _ => 8,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Prim::Float | Prim::Double)
    }

    pub fn signed(self) -> bool {
        use Prim::*;
        matches!(
            self,
            Char | SChar
                | Short
                | Int
                | Long
                | LongLong
                | WChar
                | I8
                | I16
                | I32
                | I64
                | SSize
                | IntPtr
                | PtrDiff
                | IntMax
                | Float
                | Double
        )
    }

    /// The scalar of the common FFI with the same representation.
    pub fn cty(self) -> CTy {
        match (self, self.size(), self.signed()) {
            (Prim::Float, ..) => CTy::F32,
            (Prim::Double, ..) => CTy::F64,
            (_, 1, true) => CTy::I8,
            (_, 1, false) => CTy::U8,
            (_, 2, true) => CTy::I16,
            (_, 2, false) => CTy::U16,
            (_, 4, true) => CTy::I32,
            (_, 4, false) => CTy::U32,
            (_, _, true) => CTy::I64,
            (_, _, false) => CTy::U64,
        }
    }

    /// The unsigned integer of `size` bytes (bit-field storage units).
    pub fn unsigned_of(size: u64) -> Option<Prim> {
        Some(match size {
            1 => Prim::U8,
            2 => Prim::U16,
            4 => Prim::U32,
            8 => Prim::U64,
            _ => return None,
        })
    }

    /// The standard typedef `name` (`uint32_t`, `size_t`, …).
    pub fn of_typedef(name: &str) -> Option<Prim> {
        use Prim::*;
        Some(match name {
            "int8_t" => I8,
            "uint8_t" => U8,
            "int16_t" => I16,
            "uint16_t" | "char16_t" => U16,
            "int32_t" => I32,
            "uint32_t" | "char32_t" => U32,
            "int64_t" => I64,
            "uint64_t" => U64,
            "size_t" => Size,
            "ssize_t" => SSize,
            "intptr_t" => IntPtr,
            "uintptr_t" => UIntPtr,
            "ptrdiff_t" => PtrDiff,
            "intmax_t" => IntMax,
            "uintmax_t" => UIntMax,
            "wchar_t" => WChar,
            _ => return None,
        })
    }
}

/// A C type.
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Void,
    Prim(Prim),
    Ptr(Box<Ty>),
    /// A pointer to a function: the function type ([`Ty::Fn`]) or an alias
    /// of one.
    FunPtr(Box<Ty>),
    /// A function type (behind a pointer, or named by a typedef).
    Fn(Box<Sig>),
    /// A fixed-size array (fields).
    Array(Box<Ty>, u64),
    /// A declaration of the header, by key.
    Named(String),
    /// A type no output language can represent, with a description.
    Unsupported(String),
}

impl Ty {
    /// The first unsupported part of the type.
    pub fn unsupported(&self) -> Option<&str> {
        match self {
            Ty::Unsupported(s) => Some(s),
            Ty::Ptr(t) | Ty::FunPtr(t) | Ty::Array(t, _) => t.unsupported(),
            Ty::Fn(s) => s.unsupported(),
            _ => None,
        }
    }

    /// The keys of the declarations the type refers to.
    pub fn refs(&self, out: &mut Vec<String>) {
        match self {
            Ty::Named(k) => out.push(k.clone()),
            Ty::Ptr(t) | Ty::FunPtr(t) | Ty::Array(t, _) => t.refs(out),
            Ty::Fn(s) => {
                s.ret.refs(out);
                for p in &s.params {
                    p.refs(out);
                }
            }
            _ => {}
        }
    }
}

/// A function type.
#[derive(Clone, Debug, PartialEq)]
pub struct Sig {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub variadic: bool,
}

impl Sig {
    pub fn unsupported(&self) -> Option<&str> {
        self.params
            .iter()
            .chain(std::iter::once(&self.ret))
            .find_map(|t| t.unsupported())
    }
}

/// A declaration.
#[derive(Clone, Debug)]
pub struct Item {
    pub key: String,
    /// The C name.
    pub name: String,
    /// The file that declares it (empty for built-in declarations).
    pub file: String,
    pub line: u32,
    pub def: Def,
}

#[derive(Clone, Debug)]
pub enum Def {
    /// A struct or union without a usable definition: only declared
    /// (`struct VkInstance_T`), or one whose layout cannot be represented
    /// (the reason).
    Opaque(Option<String>),
    Record(Record),
    Enum(Enum),
    /// A typedef.
    Alias(Ty),
    Const(Const),
    Function(Function),
    /// A declaration that is not output, with the reason (shown as a
    /// comment).
    Skipped(String),
}

/// A struct or union whose layout was checked against clang's.
#[derive(Clone, Debug)]
pub struct Record {
    pub union: bool,
    pub packed: bool,
    pub size: u64,
    pub align: u64,
    pub fields: Vec<Field>,
    /// Remarks shown as comments (omitted flexible array members).
    pub notes: Vec<String>,
}

/// A field at a byte offset. A bit-field storage unit has the bit-fields
/// it holds in `bits`.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub offset: u64,
    pub bits: Vec<BitField>,
}

/// A bit-field: its position in the storage unit and its width.
#[derive(Clone, Debug)]
pub struct BitField {
    pub name: String,
    pub offset: u32,
    pub width: u32,
}

#[derive(Clone, Debug)]
pub struct Enum {
    /// The integer type of the enum.
    pub int: Prim,
    pub consts: Vec<(String, i128)>,
}

#[derive(Clone, Debug)]
pub struct Const {
    pub ty: Ty,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i128),
    Float(f64),
    Str(String),
}

#[derive(Clone, Debug)]
pub struct Function {
    /// Parameter names (`None` when unnamed or reserved).
    pub names: Vec<Option<String>>,
    pub sig: Sig,
}

/// The declarations selected for output, in source order.
#[derive(Clone, Debug, Default)]
pub struct Header {
    pub items: Vec<Item>,
    by_key: HashMap<String, usize>,
}

impl Header {
    pub fn new(items: Vec<Item>) -> Header {
        let by_key = items
            .iter()
            .enumerate()
            .map(|(i, it)| (it.key.clone(), i))
            .collect();
        Header { items, by_key }
    }

    pub fn get(&self, key: &str) -> Option<&Item> {
        self.by_key.get(key).map(|&i| &self.items[i])
    }

    /// `t` with aliases replaced by what they name (not through records).
    pub fn resolve<'a>(&'a self, t: &'a Ty) -> &'a Ty {
        let mut t = t;
        for _ in 0..64 {
            match t {
                Ty::Named(k) => match self.get(k).map(|i| &i.def) {
                    Some(Def::Alias(u)) => t = u,
                    _ => return t,
                },
                _ => return t,
            }
        }
        t
    }

    /// The integer type of `t` when it is one (through aliases and enums).
    pub fn prim(&self, t: &Ty) -> Option<Prim> {
        match self.resolve(t) {
            Ty::Prim(p) => Some(*p),
            Ty::Named(k) => match self.get(k).map(|i| &i.def) {
                Some(Def::Enum(e)) => Some(e.int),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `t` is a struct or union passed by value.
    pub fn is_record(&self, t: &Ty) -> bool {
        match self.resolve(t) {
            Ty::Named(k) => matches!(
                self.get(k).map(|i| &i.def),
                Some(Def::Record(_) | Def::Opaque(_))
            ),
            _ => false,
        }
    }

    /// The function type `t` names, through aliases.
    pub fn sig<'a>(&'a self, t: &'a Ty) -> Option<&'a Sig> {
        match self.resolve(t) {
            Ty::Fn(s) => Some(s),
            _ => None,
        }
    }
}

/// Reserved C names (`__x`, `_X`): the implementation's, never output on
/// their own.
pub fn reserved(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 2 && b[0] == b'_' && (b[1] == b'_' || b[1].is_ascii_uppercase())
}

/// `|`- or `,`-separated glob patterns (`*` any sequence, `?` one
/// character), matched against a whole name or path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Globs(Vec<String>);

impl Globs {
    pub fn parse(s: &str) -> Globs {
        Globs(
            s.split(['|', ','])
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect(),
        )
    }

    pub fn matches(&self, s: &str) -> bool {
        self.0.iter().any(|p| glob(p.as_bytes(), s.as_bytes()))
    }
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    // Iterative matching with backtracking to the last `*`.
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, si));
            pi += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_whole_names() {
        let g = Globs::parse("vk*|VK_API_VERSION_?_?, sin");
        assert!(g.matches("vkCreateInstance"));
        assert!(g.matches("VK_API_VERSION_1_0"));
        assert!(g.matches("sin"));
        assert!(!g.matches("sinf"));
        assert!(!g.matches("VK_API_VERSION_1_10"));
        assert!(Globs::parse("*/math.h").matches("/usr/include/math.h"));
        assert!(Globs::parse("a*b*c").matches("aXbYbc"));
        assert!(!Globs::parse("a*b*c").matches("aXbYbd"));
    }

    #[test]
    fn reserved_names() {
        assert!(reserved("__x") && reserved("_IO_FILE"));
        assert!(!reserved("_x") && !reserved("x_") && !reserved("_"));
    }

    #[test]
    fn prims_match_the_abi() {
        assert_eq!(Prim::of_typedef("uint32_t").map(Prim::cty), Some(CTy::U32));
        assert_eq!(Prim::Long.cty(), CTy::I64);
        assert_eq!(Prim::Bool.cty(), CTy::U8);
        assert_eq!(Prim::Size.cty(), CTy::U64);
        assert_eq!(Prim::Float.cty(), CTy::F32);
    }
}
