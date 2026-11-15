//! C types at the FFI boundary (FEAT-0047, 08-ffi.md §7.2): the structs
//! and unions of a program, their layout, and how the x86_64 System V ABI
//! passes them. The backend and the interpreter share the classification,
//! and frontends use the layout for the offsets of fields.

use crate::expr::{CSig, CTy, ExternKind};

/// A C type of a signature or of a field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CType {
    Scalar(CTy),
    /// A struct or union of the program, by name.
    Agg(String),
    /// A fixed-size array (fields only).
    Array(Box<CType>, u32),
}

impl From<CTy> for CType {
    fn from(t: CTy) -> CType {
        CType::Scalar(t)
    }
}

impl CType {
    pub fn scalar(&self) -> Option<CTy> {
        match self {
            CType::Scalar(t) => Some(*t),
            _ => None,
        }
    }

    pub fn is_void(&self) -> bool {
        *self == CType::Scalar(CTy::Void)
    }

    pub fn is_agg(&self) -> bool {
        matches!(self, CType::Agg(_))
    }
}

impl std::fmt::Display for CType {
    /// The Core text form (`i32`, `timespec`, `(array f32 4)`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CType::Scalar(t) => f.write_str(t.name()),
            CType::Agg(n) => f.write_str(n),
            CType::Array(t, n) => write!(f, "(array {t} {n})"),
        }
    }
}

impl CTy {
    /// Size in bytes (also the alignment); 0 for `void`.
    pub fn size(self) -> u64 {
        match self {
            CTy::I8 | CTy::U8 => 1,
            CTy::I16 | CTy::U16 => 2,
            CTy::I32 | CTy::U32 | CTy::F32 => 4,
            CTy::I64 | CTy::U64 | CTy::F64 | CTy::Ptr => 8,
            CTy::Void => 0,
        }
    }
}

/// A struct or union declared by the program (`cstruct` / `cunion`).
#[derive(Clone, Debug, PartialEq)]
pub struct CAgg {
    pub name: String,
    pub union: bool,
    /// `__attribute__((packed))`: every field at alignment 1.
    pub packed: bool,
    pub fields: Vec<CType>,
}

/// The layout of an aggregate: size and alignment in bytes and the offset
/// of each field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub size: u64,
    pub align: u64,
    pub offsets: Vec<u64>,
}

/// One eightbyte of an aggregate passed in a register, as the LLVM type
/// it is moved as (clang's coercion).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    /// An integer register with the eightbyte's bytes (1 to 8; fewer than 8
    /// only in the last eightbyte).
    Int(u8),
    /// An SSE register with one `double`.
    Double,
    /// An SSE register with one `float` (a last eightbyte of 4 bytes).
    Float,
    /// An SSE register with two `float`s.
    Floats,
}

impl Piece {
    pub fn is_sse(self) -> bool {
        !matches!(self, Piece::Int(_))
    }

    /// The bytes of the aggregate the piece holds.
    pub fn bytes(self) -> u64 {
        match self {
            Piece::Int(n) => u64::from(n),
            Piece::Double | Piece::Floats => 8,
            Piece::Float => 4,
        }
    }

    /// The LLVM type the piece is moved as.
    pub fn llvm(self) -> String {
        match self {
            Piece::Int(n) => format!("i{}", u32::from(n) * 8),
            Piece::Double => "double".into(),
            Piece::Float => "float".into(),
            Piece::Floats => "<2 x float>".into(),
        }
    }
}

/// How a value of a C type is passed or returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pass {
    /// A scalar (or `void`), as itself.
    Scalar(CTy),
    /// An aggregate in registers, one piece per eightbyte.
    Regs(Vec<Piece>),
    /// An aggregate in memory: copied to the stack (an argument, `byval`) or
    /// written through a hidden pointer (a result, `sret`).
    Memory,
}

/// The integer and SSE argument registers of System V.
pub const INT_REGS: usize = 6;
pub const SSE_REGS: usize = 8;

/// Nesting deeper than this is an error (and stops runaway recursion).
const MAX_DEPTH: u32 = 64;

/// The C aggregates of a program.
#[derive(Clone, Copy, Debug)]
pub struct CTypes<'a>(pub &'a [CAgg]);

impl<'a> CTypes<'a> {
    pub fn get(&self, name: &str) -> Option<&'a CAgg> {
        self.0.iter().find(|a| a.name == name)
    }

    /// Size and alignment of `t`; `None` for `void`, an unknown name or a
    /// recursive aggregate.
    pub fn size_align(&self, t: &CType) -> Option<(u64, u64)> {
        self.size_align_at(t, 0)
    }

    fn size_align_at(&self, t: &CType, depth: u32) -> Option<(u64, u64)> {
        match t {
            CType::Scalar(CTy::Void) => None,
            CType::Scalar(s) => Some((s.size(), s.size())),
            CType::Array(e, n) => {
                let (s, a) = self.size_align_at(e, depth + 1)?;
                Some((s.checked_mul(u64::from(*n))?, a))
            }
            CType::Agg(name) => {
                let l = self.layout_at(name, depth + 1)?;
                Some((l.size, l.align))
            }
        }
    }

    /// The layout of the aggregate `name` (as gcc and clang lay it out).
    pub fn layout(&self, name: &str) -> Option<Layout> {
        self.layout_at(name, 0)
    }

    fn layout_at(&self, name: &str, depth: u32) -> Option<Layout> {
        if depth > MAX_DEPTH {
            return None;
        }
        let a = self.get(name)?;
        if a.fields.is_empty() {
            return None;
        }
        let mut offsets = Vec::with_capacity(a.fields.len());
        let (mut end, mut align) = (0u64, 1u64);
        for f in &a.fields {
            let (s, fa) = self.size_align_at(f, depth + 1)?;
            let fa = if a.packed { 1 } else { fa };
            align = align.max(fa);
            if a.union {
                offsets.push(0);
                end = end.max(s);
            } else {
                let off = end.next_multiple_of(fa);
                offsets.push(off);
                end = off.checked_add(s)?;
            }
        }
        Some(Layout {
            size: end.next_multiple_of(align),
            align,
            offsets,
        })
    }

    /// The scalars of `t` with their offsets from `base`.
    fn leaves(&self, t: &CType, base: u64, out: &mut Vec<(u64, CTy)>, depth: u32) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        match t {
            CType::Scalar(s) => out.push((base, *s)),
            CType::Array(e, n) => {
                let (s, _) = self.size_align_at(e, depth + 1)?;
                for i in 0..u64::from(*n) {
                    self.leaves(e, base + i * s, out, depth + 1)?;
                }
            }
            CType::Agg(name) => {
                let l = self.layout_at(name, depth + 1)?;
                for (f, off) in self.get(name)?.fields.iter().zip(&l.offsets) {
                    self.leaves(f, base + off, out, depth + 1)?;
                }
            }
        }
        Some(())
    }

    /// The System V class of a value of `t` on its own (before registers
    /// run out). An aggregate of at most 16 bytes whose fields are aligned
    /// goes in registers: an eightbyte with an integer or pointer in an
    /// integer register, one with floating point only in an SSE register.
    /// Anything else goes in memory. `None` for an unknown aggregate.
    pub fn classify(&self, t: &CType) -> Option<Pass> {
        let name = match t {
            CType::Scalar(s) => return Some(Pass::Scalar(*s)),
            CType::Agg(n) => n,
            CType::Array(..) => return None,
        };
        let l = self.layout(name)?;
        if l.size > 16 {
            return Some(Pass::Memory);
        }
        let mut leaves = Vec::new();
        self.leaves(t, 0, &mut leaves, 0)?;
        if leaves.iter().any(|(o, s)| o % s.size() != 0) {
            return Some(Pass::Memory);
        }
        let mut pieces = Vec::new();
        for k in 0..l.size.div_ceil(8) {
            let lo = 8 * k;
            let here: Vec<CTy> = leaves
                .iter()
                .filter(|(o, s)| *o < lo + 8 && o + s.size() > lo)
                .map(|(_, s)| *s)
                .collect();
            let rest = (l.size - lo).min(8);
            let sse = !here.is_empty() && here.iter().all(|s| s.is_float());
            pieces.push(if !sse {
                Piece::Int(rest as u8)
            } else if rest <= 4 {
                Piece::Float
            } else if here == [CTy::F64] {
                Piece::Double
            } else {
                Piece::Floats
            });
        }
        Some(Pass::Regs(pieces))
    }

    /// How each parameter of a call is passed, and the result. An aggregate
    /// that does not fit in the registers left is passed in memory, as
    /// clang does; a result in memory takes an integer register for its
    /// address.
    pub fn assign(&self, params: &[CType], ret: &CType) -> Option<(Vec<Pass>, Pass)> {
        let ret = self.classify(ret)?;
        let mut ints = INT_REGS - usize::from(ret == Pass::Memory);
        let mut sses = SSE_REGS;
        let mut out = Vec::with_capacity(params.len());
        for p in params {
            out.push(match self.classify(p)? {
                Pass::Scalar(s) => {
                    if s.is_float() {
                        sses = sses.saturating_sub(1);
                    } else {
                        ints = ints.saturating_sub(1);
                    }
                    Pass::Scalar(s)
                }
                Pass::Regs(ps) => {
                    let ns = ps.iter().filter(|p| p.is_sse()).count();
                    let ni = ps.len() - ns;
                    if ni <= ints && ns <= sses {
                        ints -= ni;
                        sses -= ns;
                        Pass::Regs(ps)
                    } else {
                        Pass::Memory
                    }
                }
                Pass::Memory => Pass::Memory,
            });
        }
        Some((out, ret))
    }

    /// Checks the declarations of the aggregates: unique names that are not
    /// scalar types, at least one field, known field types, arrays of at
    /// least one element and no aggregate containing itself.
    pub fn check_decls(&self) -> Vec<(String, String)> {
        let mut errs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in self.0 {
            let what = format!("C type `{}`", a.name);
            if !seen.insert(a.name.as_str()) {
                errs.push((what, "defined twice".into()));
                continue;
            }
            if CTy::from_name(&a.name).is_some() || a.name == "..." {
                errs.push((what, "is the name of a C scalar type".into()));
                continue;
            }
            if a.fields.is_empty() {
                errs.push((what, "has no fields".into()));
                continue;
            }
            for f in &a.fields {
                if let Err(m) = self.check_field(f) {
                    errs.push((what.clone(), m));
                }
            }
            if a.fields.iter().any(|f| self.reaches(f, &a.name, 0)) {
                errs.push((what, "contains itself".into()));
            }
        }
        errs
    }

    /// `t` contains the aggregate `name` by value.
    fn reaches(&self, t: &CType, name: &str, depth: u32) -> bool {
        // A cycle not through `name` is reported for its own members.
        if depth > MAX_DEPTH {
            return false;
        }
        match t {
            CType::Scalar(_) => false,
            CType::Array(e, _) => self.reaches(e, name, depth + 1),
            CType::Agg(n) => {
                n == name
                    || self
                        .get(n)
                        .is_some_and(|a| a.fields.iter().any(|f| self.reaches(f, name, depth + 1)))
            }
        }
    }

    fn check_field(&self, t: &CType) -> Result<(), String> {
        match t {
            CType::Scalar(CTy::Void) => Err("a field cannot be `void`".into()),
            CType::Scalar(_) => Ok(()),
            CType::Array(e, n) => {
                if *n == 0 {
                    return Err("an array has at least one element".into());
                }
                self.check_field(e)
            }
            CType::Agg(n) => match self.get(n) {
                Some(_) => Ok(()),
                None => Err(format!("unknown C type `{n}`")),
            },
        }
    }

    /// Checks a C signature on its own: known aggregates, `void` only as the
    /// result, no arrays, promoted variadic arguments, and callbacks that
    /// are not variadic.
    pub fn check_sig(&self, sig: &CSig, kind: &ExternKind) -> Result<(), String> {
        for (i, p) in sig.params.iter().enumerate() {
            self.check_value(p)?;
            if p.is_void() {
                return Err("a parameter cannot be `void`".into());
            }
            if sig.variadic.is_some_and(|n| i >= n)
                && let CType::Scalar(s) = p
                && matches!(s, CTy::F32 | CTy::I8 | CTy::U8 | CTy::I16 | CTy::U16)
            {
                return Err(format!(
                    "a variadic argument cannot be `{}` (C promotes it to `{}`)",
                    s.name(),
                    if *s == CTy::F32 { "f64" } else { "i32" }
                ));
            }
        }
        self.check_value(&sig.ret)?;
        if sig.variadic.is_some_and(|n| n > sig.params.len()) {
            return Err("more fixed parameters than parameters".into());
        }
        if sig.variadic.is_some() && matches!(kind, ExternKind::Callback(_)) {
            return Err("a callback cannot be variadic".into());
        }
        Ok(())
    }

    fn check_value(&self, t: &CType) -> Result<(), String> {
        match t {
            CType::Scalar(_) => Ok(()),
            CType::Array(..) => Err(format!(
                "an array `{t}` cannot be passed by value (pass a pointer)"
            )),
            CType::Agg(n) => match self.layout(n) {
                Some(_) => Ok(()),
                None if self.get(n).is_some() => Err(format!("C type `{n}` has no layout")),
                None => Err(format!("unknown C type `{n}`")),
            },
        }
    }
}

/// An extern as [`check_links`] sees it.
pub struct LinkUse<'a> {
    pub name: &'a str,
    pub symbol: &'a str,
    pub kind: &'a ExternKind,
    pub sig: Option<&'a CSig>,
}

/// The externs sharing a C symbol agree: one LLVM declaration serves them.
/// Calls of a variadic function agree on the fixed parameters and the
/// result, other calls on the whole signature; a data symbol (`:addr`
/// without a signature) is not also a function.
pub fn check_links(uses: &[LinkUse<'_>]) -> Vec<(String, String)> {
    let mut errs = Vec::new();
    let mut first: std::collections::HashMap<&str, &LinkUse<'_>> = Default::default();
    let mut func: std::collections::HashMap<&str, bool> = Default::default();
    for u in uses {
        let is_func = match u.kind {
            ExternKind::Dynamic | ExternKind::Callback(_) => continue,
            ExternKind::Addr => u.sig.is_some(),
            ExternKind::Call => true,
        };
        if let Some(f) = func.insert(u.symbol, is_func)
            && f != is_func
        {
            errs.push((
                format!("extern `{}`", u.name),
                format!("`{}` is used both as data and as a function", u.symbol),
            ));
            continue;
        }
        // Addresses need no particular declaration; externs of the runtime
        // convention (no C signature) have unique names.
        let (ExternKind::Call, Some(sig)) = (u.kind, u.sig) else {
            continue;
        };
        match first.get(u.symbol) {
            None => {
                first.insert(u.symbol, u);
            }
            Some(f) => {
                let fs = f.sig.unwrap();
                let agree = match (fs.variadic, sig.variadic) {
                    (Some(a), Some(b)) => {
                        a == b && fs.params[..a] == sig.params[..b] && fs.ret == sig.ret
                    }
                    (None, None) => fs == sig,
                    _ => false,
                };
                if !agree {
                    errs.push((
                        format!("extern `{}`", u.name),
                        format!(
                            "C function `{}` is declared with another signature by `{}`",
                            u.symbol, f.name
                        ),
                    ));
                }
            }
        }
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agg(name: &str, fields: Vec<CType>) -> CAgg {
        CAgg {
            name: name.into(),
            union: false,
            packed: false,
            fields,
        }
    }

    fn s(t: CTy) -> CType {
        CType::Scalar(t)
    }

    fn table() -> Vec<CAgg> {
        use CTy::*;
        vec![
            agg("timespec", vec![s(I64), s(I64)]),
            agg("div", vec![s(I32), s(I32)]),
            agg("vec3", vec![s(F32), s(F32), s(F32)]),
            agg("dd", vec![s(F64), s(F64)]),
            agg("df", vec![s(F64), s(F32)]),
            agg("mixed", vec![s(F32), s(I32), s(F64)]),
            agg("three", vec![s(I8), s(I8), s(I8)]),
            agg("big", vec![s(I64), s(I64), s(I64)]),
            agg("pad", vec![s(I8), s(F64)]),
            agg("nest", vec![CType::Agg("div".into()), s(F32)]),
            agg("arr", vec![CType::Array(Box::new(s(F32)), 3), s(I16)]),
            CAgg {
                name: "uni".into(),
                union: true,
                packed: false,
                fields: vec![
                    CType::Array(Box::new(s(F32)), 4),
                    CType::Array(Box::new(s(I32)), 4),
                ],
            },
            CAgg {
                name: "ufd".into(),
                union: true,
                packed: false,
                fields: vec![s(F32), s(F64)],
            },
            CAgg {
                name: "packed".into(),
                union: false,
                packed: true,
                fields: vec![s(U8), s(U32)],
            },
            agg("self", vec![CType::Agg("self".into())]),
            agg("empty", vec![]),
        ]
    }

    #[test]
    fn layouts_follow_the_c_rules() {
        let t = table();
        let c = CTypes(&t);
        let l = |n: &str| c.layout(n).unwrap();
        assert_eq!(
            l("timespec"),
            Layout {
                size: 16,
                align: 8,
                offsets: vec![0, 8]
            }
        );
        assert_eq!(
            l("vec3"),
            Layout {
                size: 12,
                align: 4,
                offsets: vec![0, 4, 8]
            }
        );
        assert_eq!(
            l("df"),
            Layout {
                size: 16,
                align: 8,
                offsets: vec![0, 8]
            }
        );
        assert_eq!(
            l("pad"),
            Layout {
                size: 16,
                align: 8,
                offsets: vec![0, 8]
            }
        );
        assert_eq!(
            l("three"),
            Layout {
                size: 3,
                align: 1,
                offsets: vec![0, 1, 2]
            }
        );
        assert_eq!(
            l("nest"),
            Layout {
                size: 12,
                align: 4,
                offsets: vec![0, 8]
            }
        );
        assert_eq!(
            l("arr"),
            Layout {
                size: 16,
                align: 4,
                offsets: vec![0, 12]
            }
        );
        assert_eq!(
            l("uni"),
            Layout {
                size: 16,
                align: 4,
                offsets: vec![0, 0]
            }
        );
        assert_eq!(
            l("packed"),
            Layout {
                size: 5,
                align: 1,
                offsets: vec![0, 1]
            }
        );
        assert_eq!(c.layout("self"), None);
        assert_eq!(c.layout("empty"), None);
        assert_eq!(c.layout("nothing"), None);
        assert_eq!(
            c.size_align(&CType::Array(Box::new(s(CTy::U16)), 5)),
            Some((10, 2))
        );
    }

    #[test]
    fn classification_matches_clang() {
        use Piece::*;
        let t = table();
        let c = CTypes(&t);
        let k = |n: &str| c.classify(&CType::Agg(n.into())).unwrap();
        assert_eq!(k("timespec"), Pass::Regs(vec![Int(8), Int(8)]));
        assert_eq!(k("div"), Pass::Regs(vec![Int(8)]));
        assert_eq!(k("vec3"), Pass::Regs(vec![Floats, Float]));
        assert_eq!(k("dd"), Pass::Regs(vec![Double, Double]));
        assert_eq!(k("df"), Pass::Regs(vec![Double, Floats]));
        // An eightbyte with an integer goes in an integer register.
        assert_eq!(k("mixed"), Pass::Regs(vec![Int(8), Double]));
        assert_eq!(k("three"), Pass::Regs(vec![Int(3)]));
        assert_eq!(k("pad"), Pass::Regs(vec![Int(8), Double]));
        assert_eq!(k("nest"), Pass::Regs(vec![Int(8), Float]));
        assert_eq!(k("arr"), Pass::Regs(vec![Floats, Int(8)]));
        // A union merges the classes of its fields.
        assert_eq!(k("uni"), Pass::Regs(vec![Int(8), Int(8)]));
        assert_eq!(k("ufd"), Pass::Regs(vec![Floats]));
        assert_eq!(k("big"), Pass::Memory);
        // An unaligned field puts the aggregate in memory.
        assert_eq!(k("packed"), Pass::Memory);
        assert_eq!(c.classify(&s(CTy::I32)), Some(Pass::Scalar(CTy::I32)));
        assert_eq!(Piece::Int(3).llvm(), "i24");
        assert_eq!(Piece::Floats.llvm(), "<2 x float>");
    }

    #[test]
    fn aggregates_that_do_not_fit_go_in_memory() {
        let t = table();
        let c = CTypes(&t);
        let ts = CType::Agg("timespec".into());
        let i = s(CTy::I64);
        // Five integer registers taken: a struct needing two goes to memory,
        // the scalar after it still gets the last register.
        let ps = vec![
            i.clone(),
            i.clone(),
            i.clone(),
            i.clone(),
            i.clone(),
            ts.clone(),
            i.clone(),
        ];
        let (a, r) = c.assign(&ps, &s(CTy::Void)).unwrap();
        assert_eq!(a[5], Pass::Memory);
        assert_eq!(a[6], Pass::Scalar(CTy::I64));
        assert_eq!(r, Pass::Scalar(CTy::Void));
        // A result in memory takes the first integer register.
        let ps = vec![i.clone(), i.clone(), i.clone(), i.clone(), ts.clone()];
        let (a, r) = c.assign(&ps, &CType::Agg("big".into())).unwrap();
        assert_eq!(r, Pass::Memory);
        assert_eq!(a[4], Pass::Memory);
        let (a, _) = c.assign(&ps, &s(CTy::Void)).unwrap();
        assert_eq!(a[4], Pass::Regs(vec![Piece::Int(8), Piece::Int(8)]));
    }

    #[test]
    fn declarations_and_signatures_are_checked() {
        let t = table();
        let c = CTypes(&t);
        let errs = c.check_decls();
        let msgs: Vec<String> = errs.iter().map(|(w, m)| format!("{w}: {m}")).collect();
        assert_eq!(
            msgs,
            vec![
                "C type `self`: contains itself".to_string(),
                "C type `empty`: has no fields".to_string(),
            ]
        );
        let sig = |ps: Vec<CType>, v| CSig {
            params: ps,
            ret: s(CTy::I32),
            variadic: v,
        };
        let call = ExternKind::Call;
        assert!(
            c.check_sig(&sig(vec![s(CTy::Ptr), s(CTy::F64)], Some(1)), &call)
                .is_ok()
        );
        let e = c.check_sig(&sig(vec![s(CTy::Ptr), s(CTy::F32)], Some(1)), &call);
        assert_eq!(
            e.unwrap_err(),
            "a variadic argument cannot be `f32` (C promotes it to `f64`)"
        );
        let cb = ExternKind::Callback("h".into());
        assert!(c.check_sig(&sig(vec![s(CTy::Ptr)], Some(1)), &cb).is_err());
        assert!(
            c.check_sig(&sig(vec![CType::Agg("nothing".into())], None), &call)
                .is_err()
        );
        assert!(
            c.check_sig(
                &sig(vec![CType::Array(Box::new(s(CTy::I8)), 2)], None),
                &call
            )
            .is_err()
        );
        assert!(c.check_sig(&sig(vec![s(CTy::Void)], None), &call).is_err());
    }

    #[test]
    fn externs_of_one_symbol_agree() {
        let v = |ps: Vec<CTy>, n| CSig {
            params: ps.into_iter().map(CType::Scalar).collect(),
            ret: s(CTy::I32),
            variadic: n,
        };
        let a = v(vec![CTy::Ptr, CTy::I32], Some(1));
        let b = v(vec![CTy::Ptr, CTy::F64, CTy::F64], Some(1));
        let c = v(vec![CTy::I32], None);
        let call = ExternKind::Call;
        let addr = ExternKind::Addr;
        let u = |name, sym, kind, sig| LinkUse {
            name,
            symbol: sym,
            kind,
            sig,
        };
        assert!(
            check_links(&[
                u("p1", "printf", &call, Some(&a)),
                u("p2", "printf", &call, Some(&b)),
                u("pa", "printf", &addr, Some(&c)),
            ])
            .is_empty()
        );
        let errs = check_links(&[
            u("p1", "printf", &call, Some(&a)),
            u("p3", "printf", &call, Some(&c)),
        ]);
        assert_eq!(errs.len(), 1);
        let errs = check_links(&[
            u("e", "environ", &addr, None),
            u("f", "environ", &call, Some(&c)),
        ]);
        assert_eq!(errs.len(), 1);
    }
}
