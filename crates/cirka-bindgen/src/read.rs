//! Reading a header with libclang into the declarations of [`crate::model`]
//! (08-ffi.md §6).
//!
//! Pass 1 walks the top-level declarations of the header's translation
//! unit. Pass 2 evaluates the object-like macros: a second translation
//! unit includes the header and declares `__auto_type v = MACRO;` for each
//! one, so that clang expands and evaluates them (also when they use
//! function-like macros). Finally the layout of every struct and union is
//! computed with `cirka_core::ctype` and compared with clang's.

use crate::clang::{self, Clang, Cursor, Eval, Type, kind, ty};
use crate::model::*;
use cirka_core::{CAgg, CTy, CType, CTypes};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

/// The declarations of `header` and the files that declare them, in the
/// order they are first seen. Errors are clang's diagnostics.
pub fn read(
    clang: &Clang,
    header: &Path,
    args: &[String],
    resource_dir: Option<&str>,
) -> Result<(Vec<Item>, Vec<String>), Vec<String>> {
    let mut r = Reader {
        resource_dir: resource_dir.map(String::from),
        ..Reader::default()
    };
    {
        let unit = clang
            .parse(header, args, None, true)
            .map_err(|e| vec![format!("error: {e}")])?;
        let errors: Vec<String> = unit
            .diagnostics()
            .into_iter()
            .filter(|d| d.error)
            .map(|d| d.text)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let top = unit.cursor().children();
        r.scan_typedefs(&top);
        for c in &top {
            r.top(*c);
        }
    }
    r.check_layouts();
    r.macro_values(clang, header, args)
        .map_err(|e| vec![format!("error: {e}")])?;
    Ok((r.items, r.files))
}

#[derive(Default)]
struct Reader {
    items: Vec<Item>,
    index: HashMap<String, usize>,
    /// Typedefs that name a struct, union or enum of the same name (or one
    /// they give a name to): no alias is output for them.
    transparent: HashSet<String>,
    /// Names given by typedefs to anonymous tags and tags starting with
    /// `_` (`typedef struct _IO_FILE FILE;`), by the tag's key.
    tag_names: HashMap<String, String>,
    /// Records being read (a pointer to itself must not read it again).
    in_progress: HashSet<String>,
    functions: HashSet<String>,
    macros: Vec<(String, String, u32)>,
    resource_dir: Option<String>,
    /// Files in the order they are first seen (sorting the output).
    files: Vec<String>,
}

/// The file and line of `c`. The file name is absolute, without `.` and
/// `..` components (what `--file` patterns match).
fn location(c: Cursor<'_>) -> (String, u32) {
    let (file, line) = c.location().unwrap_or_default();
    (normalize(&file), line)
}

/// `file` as an absolute path without `.` and `..` components (lexically:
/// symbolic links are kept). An empty name stays empty.
fn normalize(file: &str) -> String {
    if file.is_empty() {
        return String::new();
    }
    let p = std::path::absolute(file).unwrap_or_else(|_| PathBuf::from(file));
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out.to_string_lossy().into_owned()
}

/// `t` without `struct` / attribute sugar.
fn strip(t: Type<'_>) -> Type<'_> {
    match t.kind() {
        ty::ELABORATED => strip(t.named()),
        ty::ATTRIBUTED => strip(t.modified()),
        _ => t,
    }
}

fn is_function(t: Type<'_>) -> bool {
    matches!(
        t.canonical().kind(),
        ty::FUNCTION_PROTO | ty::FUNCTION_NO_PROTO
    )
}

fn is_va_list(name: &str) -> bool {
    matches!(name, "va_list" | "__builtin_va_list" | "__gnuc_va_list")
}

/// The key of the struct, union or enum at the bottom of a field type
/// (through pointers and arrays).
fn base_usr(t: Type<'_>) -> Option<String> {
    let t = strip(t);
    match t.kind() {
        ty::POINTER => base_usr(t.pointee()),
        ty::CONSTANT_ARRAY | ty::INCOMPLETE_ARRAY => base_usr(t.element()),
        ty::RECORD | ty::ENUM => Some(t.declaration().usr()),
        _ => None,
    }
}

/// The first named field of an anonymous member, through nested ones.
fn first_named_field(c: Cursor<'_>) -> Option<String> {
    c.children().into_iter().find_map(|f| match f.kind() {
        kind::FIELD if !f.spelling().is_empty() => Some(f.spelling()),
        kind::STRUCT | kind::UNION if f.is_anonymous_member() => first_named_field(f),
        _ => None,
    })
}

/// A bit-field before it is put into a storage unit.
struct Bit {
    name: String,
    prim: Prim,
    offset: u64,
    width: u32,
}

impl Reader {
    fn builtin_file(&self, file: &str) -> bool {
        file.is_empty()
            || self
                .resource_dir
                .as_deref()
                .is_some_and(|d| file.starts_with(d))
    }

    fn see_file(&mut self, file: &str) {
        if !self.files.iter().any(|f| f == file) {
            self.files.push(file.to_string());
        }
    }

    fn push(&mut self, item: Item) -> usize {
        let i = self.items.len();
        self.index.insert(item.key.clone(), i);
        self.items.push(item);
        i
    }

    fn item(&self, key: &str) -> Option<&Item> {
        self.index.get(key).map(|&i| &self.items[i])
    }

    /// Finds the typedefs that name tags (before reading any declaration,
    /// as a tag can be used before the typedef that names it).
    fn scan_typedefs(&mut self, top: &[Cursor<'_>]) {
        for c in top.iter().filter(|c| c.kind() == kind::TYPEDEF) {
            let name = c.spelling();
            if reserved(&name) {
                continue;
            }
            let u = strip(c.typedef_underlying());
            if !matches!(u.kind(), ty::RECORD | ty::ENUM) {
                continue;
            }
            let d = u.declaration();
            let (usr, tag) = (d.usr(), d.spelling());
            if tag == name {
                self.transparent.insert(c.usr());
            } else if (d.is_anonymous() || tag.starts_with('_'))
                && !self.tag_names.contains_key(&usr)
            {
                self.tag_names.insert(usr, name);
                self.transparent.insert(c.usr());
            }
        }
    }

    fn top(&mut self, c: Cursor<'_>) {
        let (file, line) = location(c);
        if self.builtin_file(&file) {
            return;
        }
        self.see_file(&file);
        match c.kind() {
            kind::STRUCT | kind::UNION | kind::ENUM => {
                if self.tag(c, None).is_none() && c.kind() == kind::ENUM {
                    self.anonymous_enum(c, &file, line);
                }
            }
            kind::TYPEDEF => self.typedef(c, file, line),
            kind::FUNCTION => self.function(c, file, line),
            kind::VAR => self.var(c, file, line),
            kind::MACRO_DEFINITION => {
                let name = c.spelling();
                if !c.is_function_like_or_builtin_macro()
                    && !reserved(&name)
                    && !self.macros.iter().any(|(n, ..)| *n == name)
                {
                    self.macros.push((name, file, line));
                }
            }
            _ => {}
        }
    }

    /// Reads the struct, union or enum `c` (once), naming it `forced`, the
    /// name a typedef gives it, or its own name. Anonymous tags without a
    /// name give `None`.
    fn tag(&mut self, c: Cursor<'_>, forced: Option<String>) -> Option<String> {
        let key = c.usr();
        if let Some(&i) = self.index.get(&key) {
            let undefined = matches!(self.items[i].def, Def::Opaque(None));
            if undefined
                && !self.in_progress.contains(&key)
                && let Some(d) = c.definition()
            {
                self.define(i, d);
            }
            return Some(key);
        }
        let name = forced
            .or_else(|| self.tag_names.get(&key).cloned())
            .or_else(|| (!c.is_anonymous()).then(|| c.spelling()))?;
        let d = c.definition();
        let (file, line) = location(d.unwrap_or(c));
        let i = self.push(Item {
            key: key.clone(),
            name,
            file,
            line,
            def: Def::Opaque(None),
        });
        if let Some(d) = d {
            self.define(i, d);
        }
        Some(key)
    }

    fn define(&mut self, i: usize, d: Cursor<'_>) {
        let key = self.items[i].key.clone();
        let name = self.items[i].name.clone();
        self.in_progress.insert(key.clone());
        let def = match d.kind() {
            kind::STRUCT | kind::UNION => self.record(d, &name),
            kind::ENUM => self.enum_def(d),
            _ => Def::Opaque(None),
        };
        self.items[i].def = def;
        self.in_progress.remove(&key);
    }

    fn enum_def(&mut self, d: Cursor<'_>) -> Def {
        let it = self.convert(d.enum_int_type());
        let Some(int) = self.prim_of(&it) else {
            return Def::Opaque(Some("an enum of an unknown integer type".into()));
        };
        let consts = d
            .children()
            .into_iter()
            .filter(|c| c.kind() == kind::ENUM_CONSTANT)
            .map(|c| (c.spelling(), c.enum_value(int.signed())))
            .collect();
        Def::Enum(Enum { int, consts })
    }

    /// The constants of `enum { A, B };` (no type to output).
    fn anonymous_enum(&mut self, c: Cursor<'_>, file: &str, line: u32) {
        let it = self.convert(c.enum_int_type());
        let Some(int) = self.prim_of(&it) else {
            return;
        };
        for k in c.children() {
            let name = k.spelling();
            if k.kind() != kind::ENUM_CONSTANT || reserved(&name) {
                continue;
            }
            self.push(Item {
                key: format!("const:{name}"),
                name,
                file: file.to_string(),
                line,
                def: Def::Const(Const {
                    ty: Ty::Prim(int),
                    value: Value::Int(k.enum_value(int.signed())),
                }),
            });
        }
    }

    fn record(&mut self, d: Cursor<'_>, name: &str) -> Def {
        let t = d.ty();
        let (Some(size), Some(align)) = (t.size(), t.align()) else {
            return Def::Opaque(Some("clang gives no size".into()));
        };
        let union = d.kind() == kind::UNION;
        let children = d.children();
        let packed = children.iter().any(|c| c.kind() == kind::PACKED_ATTR);
        let mut fields: Vec<Field> = Vec::new();
        let mut notes = Vec::new();
        let mut run: Vec<Bit> = Vec::new();
        let (mut anon, mut units) = (0, 0);
        for (i, c) in children.iter().enumerate() {
            match c.kind() {
                kind::STRUCT | kind::UNION | kind::ENUM if c.is_anonymous_member() => {
                    match Self::bit_units(&mut run, union, &mut units) {
                        Ok(f) => fields.extend(f),
                        Err(e) => return Def::Opaque(Some(e)),
                    }
                    let fname = format!("anon{anon}");
                    anon += 1;
                    let k = self.tag(*c, Some(format!("{name}_{fname}")));
                    let off = first_named_field(*c).and_then(|leaf| {
                        let outer = t.offset_of(&leaf)?;
                        let inner = c.ty().offset_of(&leaf)?;
                        let o = outer.checked_sub(inner)?;
                        (o % 8 == 0).then_some(o / 8)
                    });
                    match (k, off) {
                        (Some(k), Some(offset)) => fields.push(Field {
                            name: fname,
                            ty: Ty::Named(k),
                            offset,
                            bits: Vec::new(),
                        }),
                        _ => {
                            return Def::Opaque(Some(format!(
                                "the anonymous member {fname} has no known offset"
                            )));
                        }
                    }
                }
                kind::STRUCT | kind::UNION | kind::ENUM if c.is_anonymous() => {
                    // Named after the first field of its type.
                    let usr = c.usr();
                    if let Some(f) = children[i + 1..].iter().find(|f| {
                        f.kind() == kind::FIELD && base_usr(f.ty()).as_ref() == Some(&usr)
                    }) {
                        self.tag(*c, Some(format!("{name}_{}", f.spelling())));
                    }
                }
                kind::STRUCT | kind::UNION | kind::ENUM => {
                    // A tag declared inside a struct is at file scope in C.
                    self.tag(*c, None);
                }
                kind::FIELD => {
                    let fname = c.spelling();
                    let ft = c.ty();
                    if strip(ft).kind() == ty::INCOMPLETE_ARRAY {
                        notes.push(format!(
                            "the flexible array member `{fname}` is not part of the type"
                        ));
                        continue;
                    }
                    let Some(off) = c.field_offset_bits() else {
                        return Def::Opaque(Some(format!("clang gives no offset for `{fname}`")));
                    };
                    let fty = self.convert(ft);
                    if let Some(width) = c.bit_width() {
                        match self.prim_of(&fty) {
                            Some(prim) if !prim.is_float() => run.push(Bit {
                                name: fname,
                                prim,
                                offset: off,
                                width,
                            }),
                            _ => {
                                return Def::Opaque(Some(format!(
                                    "the bit-field `{fname}` is not of an integer type"
                                )));
                            }
                        }
                        continue;
                    }
                    match Self::bit_units(&mut run, union, &mut units) {
                        Ok(f) => fields.extend(f),
                        Err(e) => return Def::Opaque(Some(e)),
                    }
                    if let Some(u) = fty.unsupported() {
                        return Def::Opaque(Some(format!("the field `{fname}` has type {u}")));
                    }
                    if off % 8 != 0 {
                        return Def::Opaque(Some(format!("`{fname}` is not at a byte offset")));
                    }
                    fields.push(Field {
                        name: fname,
                        ty: fty,
                        offset: off / 8,
                        bits: Vec::new(),
                    });
                }
                _ => {}
            }
        }
        match Self::bit_units(&mut run, union, &mut units) {
            Ok(f) => fields.extend(f),
            Err(e) => return Def::Opaque(Some(e)),
        }
        if fields.is_empty() {
            return Def::Opaque(Some("no fields".into()));
        }
        Def::Record(Record {
            union,
            packed,
            size,
            align,
            fields,
            notes,
        })
    }

    /// Puts the bit-fields of `run` into storage units: one unsigned
    /// integer of the bit-fields' size for each aligned unit they occupy.
    fn bit_units(run: &mut Vec<Bit>, union: bool, units: &mut usize) -> Result<Vec<Field>, String> {
        if run.is_empty() {
            return Ok(Vec::new());
        }
        if union {
            return Err("a union with bit-fields".into());
        }
        let size = run[0].prim.size();
        if run.iter().any(|b| b.prim.size() != size) {
            return Err("bit-fields of types of different sizes".into());
        }
        let unit_bits = 8 * size;
        let storage = Prim::unsigned_of(size).ok_or("a bit-field of an unknown size")?;
        let mut out: Vec<Field> = Vec::new();
        for b in run.drain(..) {
            if b.width == 0 {
                continue;
            }
            let u = b.offset / unit_bits;
            if (b.offset + u64::from(b.width) - 1) / unit_bits != u {
                return Err(format!(
                    "the bit-field `{}` straddles a storage unit",
                    b.name
                ));
            }
            let offset = u * size;
            if out.last().is_none_or(|f| f.offset != offset) {
                out.push(Field {
                    name: format!("bitfield{units}"),
                    ty: Ty::Prim(storage),
                    offset,
                    bits: Vec::new(),
                });
                *units += 1;
            }
            if !b.name.is_empty() {
                let unit = out.last_mut().expect("a unit was pushed");
                unit.bits.push(BitField {
                    name: b.name,
                    offset: (b.offset - u * unit_bits) as u32,
                    width: b.width,
                });
            }
        }
        Ok(out)
    }

    fn typedef(&mut self, c: Cursor<'_>, file: String, line: u32) {
        let name = c.spelling();
        let usr = c.usr();
        if reserved(&name)
            || is_va_list(&name)
            || Prim::of_typedef(&name).is_some()
            || self.transparent.contains(&usr)
            || self.index.contains_key(&usr)
        {
            return;
        }
        let t = self.convert(c.typedef_underlying());
        let def = match t.unsupported() {
            Some(u) => Def::Skipped(format!("a typedef of {u}")),
            None => Def::Alias(t),
        };
        self.push(Item {
            key: usr,
            name,
            file,
            line,
            def,
        });
    }

    fn function(&mut self, c: Cursor<'_>, file: String, line: u32) {
        let name = c.spelling();
        if reserved(&name) || !self.functions.insert(name.clone()) {
            return;
        }
        let def = if c.storage_class() == clang::STORAGE_STATIC {
            Def::Skipped("a static function (no symbol)".into())
        } else {
            let sig = self.sig(c.ty());
            match sig.unsupported() {
                Some(u) => Def::Skipped(format!("uses {u}")),
                None => {
                    let names = c
                        .arguments()
                        .iter()
                        .map(|a| {
                            let n = a.spelling();
                            (!n.is_empty() && !reserved(&n)).then_some(n)
                        })
                        .collect();
                    Def::Function(Function { names, sig })
                }
            }
        };
        self.push(Item {
            key: c.usr(),
            name,
            file,
            line,
            def,
        });
    }

    fn var(&mut self, c: Cursor<'_>, file: String, line: u32) {
        let name = c.spelling();
        if reserved(&name) {
            return;
        }
        let t = self.convert(c.ty());
        let value = if c.storage_class() == clang::STORAGE_STATIC {
            self.constant(&t, c.evaluate())
        } else {
            None
        };
        let def = match value {
            Some(value) => Def::Const(Const { ty: t, value }),
            None if c.storage_class() == clang::STORAGE_STATIC => {
                Def::Skipped("a static variable that is not a constant".into())
            }
            None => Def::Skipped("a global variable".into()),
        };
        self.push(Item {
            key: c.usr(),
            name,
            file,
            line,
            def,
        });
    }

    /// The value of a constant of type `t`, if it is an integer of an
    /// integer type or a finite number of a floating-point type.
    fn constant(&self, t: &Ty, v: Option<Eval>) -> Option<Value> {
        let p = self.prim_of(t)?;
        match v? {
            Eval::Int(i) if !p.is_float() => Some(Value::Int(i)),
            Eval::Float(f) if p.is_float() && f.is_finite() => Some(Value::Float(f)),
            _ => None,
        }
    }

    /// The integer or floating-point type `t` is (through aliases and
    /// enums).
    fn prim_of(&self, t: &Ty) -> Option<Prim> {
        match t {
            Ty::Prim(p) => Some(*p),
            Ty::Named(k) => match &self.item(k)?.def {
                Def::Alias(u) => self.prim_of(u),
                Def::Enum(e) => Some(e.int),
                _ => None,
            },
            _ => None,
        }
    }

    fn sig(&mut self, t: Type<'_>) -> Sig {
        let t = strip(t);
        Sig {
            params: t.arg_types().into_iter().map(|a| self.param(a)).collect(),
            ret: self.convert(t.result()),
            variadic: t.is_variadic(),
        }
    }

    /// A parameter type: arrays and functions decay to pointers (libclang
    /// gives the type as written).
    fn param(&mut self, t: Type<'_>) -> Ty {
        let s = strip(t);
        let c = s.canonical();
        match (s.kind(), c.kind()) {
            (ty::CONSTANT_ARRAY | ty::INCOMPLETE_ARRAY | ty::VARIABLE_ARRAY, _) => {
                Ty::Ptr(Box::new(self.convert(s.element())))
            }
            (_, ty::CONSTANT_ARRAY | ty::INCOMPLETE_ARRAY | ty::VARIABLE_ARRAY) => {
                Ty::Ptr(Box::new(self.convert(c.element())))
            }
            (_, ty::FUNCTION_PROTO | ty::FUNCTION_NO_PROTO) => {
                Ty::FunPtr(Box::new(self.convert(t)))
            }
            _ => self.convert(t),
        }
    }

    fn convert(&mut self, t: Type<'_>) -> Ty {
        use Prim::*;
        let prim = match t.kind() {
            ty::ELABORATED => return self.convert(t.named()),
            ty::ATTRIBUTED => return self.convert(t.modified()),
            ty::VOID => return Ty::Void,
            ty::BOOL => Bool,
            ty::CHAR_U | ty::CHAR_S => Char,
            ty::SCHAR => SChar,
            ty::UCHAR => UChar,
            ty::SHORT => Short,
            ty::USHORT => UShort,
            ty::INT => Int,
            ty::UINT => UInt,
            ty::LONG => Long,
            ty::ULONG => ULong,
            ty::LONGLONG => LongLong,
            ty::ULONGLONG => ULongLong,
            ty::FLOAT => Float,
            ty::DOUBLE => Double,
            ty::WCHAR => WChar,
            ty::CHAR16 => U16,
            ty::CHAR32 => U32,
            ty::POINTER => {
                let p = t.pointee();
                let inner = Box::new(self.convert(p));
                return if is_function(p) {
                    Ty::FunPtr(inner)
                } else {
                    Ty::Ptr(inner)
                };
            }
            ty::FUNCTION_PROTO | ty::FUNCTION_NO_PROTO => return Ty::Fn(Box::new(self.sig(t))),
            ty::CONSTANT_ARRAY => {
                return match t.array_size() {
                    Some(n) if n > 0 => Ty::Array(Box::new(self.convert(t.element())), n),
                    _ => Ty::Unsupported("a zero-length array".into()),
                };
            }
            ty::TYPEDEF => return self.typedef_ref(t),
            ty::AUTO => return self.convert(t.canonical()),
            ty::RECORD | ty::ENUM => return self.tag_ref(t),
            ty::UNEXPOSED => {
                let c = t.canonical();
                return if c.kind() == ty::UNEXPOSED {
                    Ty::Unsupported(format!("`{}`", t.spelling()))
                } else {
                    self.convert(c)
                };
            }
            _ => return Ty::Unsupported(format!("`{}`", t.spelling())),
        };
        Ty::Prim(prim)
    }

    fn typedef_ref(&mut self, t: Type<'_>) -> Ty {
        let d = t.declaration();
        let name = d.spelling();
        if is_va_list(&name) {
            return Ty::Unsupported("`va_list`".into());
        }
        if let Some(p) = Prim::of_typedef(&name) {
            return Ty::Prim(p);
        }
        let usr = d.usr();
        if let Some(it) = self.item(&usr) {
            return match &it.def {
                Def::Skipped(why) => Ty::Unsupported(why.clone()),
                _ => Ty::Named(usr),
            };
        }
        // Reserved typedefs, typedefs naming tags and typedefs of built-in
        // headers are transparent.
        self.convert(d.typedef_underlying())
    }

    fn tag_ref(&mut self, t: Type<'_>) -> Ty {
        let decl = t.declaration();
        let key = decl.usr();
        if self.index.contains_key(&key) {
            return Ty::Named(key);
        }
        match self.tag(decl, None) {
            Some(k) => Ty::Named(k),
            None => Ty::Unsupported("an anonymous struct, union or enum".into()),
        }
    }

    /// Makes opaque the records whose layout `cirka_core::ctype` does not
    /// compute as clang does, until none is left (a record holding an
    /// opaque one by value becomes opaque too).
    fn check_layouts(&mut self) {
        loop {
            let mut aggs = Vec::new();
            let mut bad: Vec<(usize, String)> = Vec::new();
            for (i, it) in self.items.iter().enumerate() {
                let Def::Record(r) = &it.def else {
                    continue;
                };
                let fields: Result<Vec<CType>, String> = r
                    .fields
                    .iter()
                    .map(|f| self.ctype(&f.ty).ok_or_else(|| f.name.clone()))
                    .collect();
                match fields {
                    Ok(fields) => aggs.push(CAgg {
                        name: it.key.clone(),
                        union: r.union,
                        packed: r.packed,
                        fields,
                    }),
                    Err(f) => bad.push((i, format!("the field `{f}` has no C layout"))),
                }
            }
            let types = CTypes(&aggs);
            for (i, it) in self.items.iter().enumerate() {
                let Def::Record(r) = &it.def else {
                    continue;
                };
                if bad.iter().any(|(j, _)| *j == i) {
                    continue;
                }
                match types.layout(&it.key) {
                    Some(l)
                        if l.size == r.size
                            && l.align == r.align
                            && l.offsets.iter().eq(r.fields.iter().map(|f| &f.offset)) => {}
                    Some(l) => bad.push((
                        i,
                        format!(
                            "its layout (size {}, alignment {}) cannot be written with fields \
                             (computed: size {}, alignment {})",
                            r.size, r.align, l.size, l.align
                        ),
                    )),
                    None => bad.push((i, "a field has no C layout".into())),
                }
            }
            if bad.is_empty() {
                return;
            }
            for (i, why) in bad {
                self.items[i].def = Def::Opaque(Some(why));
            }
        }
    }

    fn ctype(&self, t: &Ty) -> Option<CType> {
        match t {
            Ty::Prim(p) => Some(CType::Scalar(p.cty())),
            Ty::Ptr(_) | Ty::FunPtr(_) => Some(CType::Scalar(CTy::Ptr)),
            Ty::Array(e, n) => Some(CType::Array(
                Box::new(self.ctype(e)?),
                u32::try_from(*n).ok()?,
            )),
            Ty::Named(k) => match &self.item(k)?.def {
                Def::Record(_) => Some(CType::Agg(k.clone())),
                Def::Enum(e) => Some(CType::Scalar(e.int.cty())),
                Def::Alias(u) => self.ctype(u),
                _ => None,
            },
            _ => None,
        }
    }

    /// Pass 2: the values of the object-like macros.
    fn macro_values(
        &mut self,
        clang: &Clang,
        header: &Path,
        args: &[String],
    ) -> Result<(), String> {
        // A macro naming a declared constant (glibc's `#define FP_NAN 0`
        // next to the enum constant) is not repeated.
        let mut declared: HashSet<&str> = self.items.iter().map(|i| i.name.as_str()).collect();
        for it in &self.items {
            if let Def::Enum(e) = &it.def {
                declared.extend(e.consts.iter().map(|(n, _)| n.as_str()));
            }
        }
        let macros: Vec<(String, String, u32)> = std::mem::take(&mut self.macros)
            .into_iter()
            .filter(|(n, ..)| !declared.contains(n.as_str()))
            .collect();
        if macros.is_empty() {
            return Ok(());
        }
        let abs = std::fs::canonicalize(header)
            .map_err(|e| format!("cannot read {}: {e}", header.display()))?;
        let mut src = String::from("#include \"");
        for ch in abs.to_string_lossy().chars() {
            if matches!(ch, '"' | '\\') {
                src.push('\\');
            }
            src.push(ch);
        }
        src.push_str("\"\n");
        for (i, (name, ..)) in macros.iter().enumerate() {
            let _ = writeln!(src, "__auto_type __cirka_bindgen_{i} = {name};");
        }
        let file = abs.with_file_name("__cirka_bindgen_macros.c");
        let unit = clang.parse(&file, args, Some(&src), false)?;
        for c in unit.cursor().children() {
            if c.kind() != kind::VAR || c.is_invalid() {
                continue;
            }
            let Some(i) = c
                .spelling()
                .strip_prefix("__cirka_bindgen_")
                .and_then(|n| n.parse::<usize>().ok())
            else {
                continue;
            };
            let Some((name, file, line)) = macros.get(i).cloned() else {
                continue;
            };
            // The type of the initializer keeps the typedef (`uint32_t`)
            // that the deduced `__auto_type` loses.
            let init = c
                .children()
                .into_iter()
                .rev()
                .find(|k| (kind::FIRST_EXPR..=kind::LAST_EXPR).contains(&k.kind()));
            let t = self.convert(init.map_or_else(|| c.ty(), |k| k.ty()));
            let string = matches!(&t, Ty::Ptr(p) if matches!(**p, Ty::Prim(Prim::Char)));
            let value = match c.evaluate() {
                Some(Eval::Str(b)) => String::from_utf8(b).ok().map(Value::Str),
                None if string => string_literal(c).map(Value::Str),
                v => self.constant(&t, v),
            };
            let Some(value) = value else {
                continue;
            };
            let ty = if matches!(value, Value::Str(_)) {
                Ty::Ptr(Box::new(Ty::Prim(Prim::Char)))
            } else {
                t
            };
            self.push(Item {
                key: format!("macro:{name}"),
                name,
                file,
                line,
                def: Def::Const(Const { ty, value }),
            });
        }
        Ok(())
    }
}

/// The string literal in the initializer of `c` (clang does not evaluate
/// a parenthesized one), decoded.
fn string_literal(c: Cursor<'_>) -> Option<String> {
    for k in c.children() {
        if k.kind() == kind::STRING_LITERAL {
            return decode_string(&k.spelling());
        }
        if let Some(s) = string_literal(k) {
            return Some(s);
        }
    }
    None
}

/// Decodes the C string literals `"…" "…"` (UTF-8, no prefix).
pub(crate) fn decode_string(src: &str) -> Option<String> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    let mut any = false;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'"' => {
                any = true;
                i += 1;
                loop {
                    match *b.get(i)? {
                        b'"' => {
                            i += 1;
                            break;
                        }
                        b'\\' => {
                            i += 1;
                            let e = *b.get(i)?;
                            i += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'r' => out.push(b'\r'),
                                b'a' => out.push(7),
                                b'b' => out.push(8),
                                b'f' => out.push(12),
                                b'v' => out.push(11),
                                b'\\' | b'"' | b'\'' | b'?' => out.push(e),
                                b'x' => {
                                    let start = i;
                                    while i < b.len() && b[i].is_ascii_hexdigit() {
                                        i += 1;
                                    }
                                    let v = u32::from_str_radix(src.get(start..i)?, 16).ok()?;
                                    out.push(u8::try_from(v).ok()?);
                                }
                                b'0'..=b'7' => {
                                    let start = i - 1;
                                    while i < b.len()
                                        && i < start + 3
                                        && (b'0'..=b'7').contains(&b[i])
                                    {
                                        i += 1;
                                    }
                                    let v = u32::from_str_radix(&src[start..i], 8).ok()?;
                                    out.push(u8::try_from(v).ok()?);
                                }
                                _ => return None,
                            }
                        }
                        c => {
                            out.push(c);
                            i += 1;
                        }
                    }
                }
            }
            _ => return None,
        }
    }
    if !any {
        return None;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::{decode_string, normalize};

    #[test]
    fn file_names_are_absolute() {
        assert_eq!(
            normalize("/usr/include/./x/../math.h"),
            "/usr/include/math.h"
        );
        assert_eq!(normalize(""), "");
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(normalize("a/../b.h"), cwd.join("b.h").to_string_lossy());
    }

    #[test]
    fn string_literals_are_decoded() {
        assert_eq!(
            decode_string(r#""a\n\x41\101\"" "b""#).as_deref(),
            Some("a\nAA\"b")
        );
        assert_eq!(decode_string("\"\\0\"").as_deref(), Some("\0"));
        assert_eq!(decode_string("L\"wide\""), None);
        assert_eq!(decode_string("\"\\q\""), None);
        assert_eq!(decode_string(""), None);
    }
}
