//! Expressions and programs of the common Core (06-ir.md §1).

use crate::ty::*;
use cirka_diagnostics::Span;

/// A term variable: printed name plus a number unique within the program.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id {
    pub name: String,
    pub id: u32,
}

impl Id {
    pub fn new(name: impl Into<String>, id: u32) -> Id {
        Id {
            name: name.into(),
            id,
        }
    }
}

/// A binding occurrence: variable, multiplicity and type.
#[derive(Clone, Debug, PartialEq)]
pub struct Binder {
    pub id: Id,
    pub mult: Mult,
    pub ty: Type,
}

impl Binder {
    pub fn new(id: Id, mult: Mult, ty: Type) -> Binder {
        Binder { id, mult, ty }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// `Int#`
    Int(i64),
    /// `Double#`
    Double(f64),
    /// `Bytes` (static, immortal)
    Str(Vec<u8>),
    /// The value of `Unit#` (written `unit`).
    Unit,
}

impl Lit {
    pub fn ty(&self) -> Type {
        match self {
            Lit::Int(_) => Type::int(),
            Lit::Double(_) => Type::double(),
            Lit::Str(_) => Type::bytes(),
            Lit::Unit => Type::unit(),
        }
    }
}

/// Primitive operations (saturated). Comparisons return `Int#` 0/1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimOp {
    Add,
    Sub,
    Mul,
    Quot,
    Rem,
    Neg,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FNeg,
    FEq,
    FLt,
    FLe,
    IntToDouble,
    DoubleToInt,
    /// `Bytes` operations.
    BytesNew,
    BytesLen,
    BytesGet,
    BytesSet,
    /// `Array a` operations (`array_set` needs a unique array).
    ArrayNew,
    ArrayLen,
    ArrayGet,
    ArraySet,
    /// `array_unique a`: consumes `a`; `a` itself when unique, otherwise a copy.
    ArrayUnique,
    /// 8-byte words of `Bytes` (unboxed arrays, FEAT-0123): `bytes_get64 b i`
    /// is the `Int#` at byte offset `8*i`; `bytes_getf` the `Double#`.
    BytesGet64,
    BytesSet64,
    BytesGetF,
    BytesSetF,
    /// `bytes_unique b`: consumes `b`; `b` itself when unique, otherwise a copy.
    BytesUnique,
    /// Stores through a mutable borrow (FEAT-0124): the object may be
    /// shared with its borrowed owner.
    ArraySetShared,
    BytesSet64Shared,
    BytesSetFShared,
    /// Pointer equality of two boxed values (`Int#`).
    PtrEq,
}

impl PrimOp {
    pub const ALL: &'static [PrimOp] = &[
        PrimOp::Add,
        PrimOp::Sub,
        PrimOp::Mul,
        PrimOp::Quot,
        PrimOp::Rem,
        PrimOp::Neg,
        PrimOp::And,
        PrimOp::Or,
        PrimOp::Xor,
        PrimOp::Shl,
        PrimOp::Shr,
        PrimOp::Eq,
        PrimOp::Ne,
        PrimOp::Lt,
        PrimOp::Le,
        PrimOp::Gt,
        PrimOp::Ge,
        PrimOp::FAdd,
        PrimOp::FSub,
        PrimOp::FMul,
        PrimOp::FDiv,
        PrimOp::FNeg,
        PrimOp::FEq,
        PrimOp::FLt,
        PrimOp::FLe,
        PrimOp::IntToDouble,
        PrimOp::DoubleToInt,
        PrimOp::BytesNew,
        PrimOp::BytesLen,
        PrimOp::BytesGet,
        PrimOp::BytesSet,
        PrimOp::ArrayNew,
        PrimOp::ArrayLen,
        PrimOp::ArrayGet,
        PrimOp::ArraySet,
        PrimOp::ArrayUnique,
        PrimOp::BytesGet64,
        PrimOp::BytesSet64,
        PrimOp::BytesGetF,
        PrimOp::BytesSetF,
        PrimOp::BytesUnique,
        PrimOp::ArraySetShared,
        PrimOp::BytesSet64Shared,
        PrimOp::BytesSetFShared,
        PrimOp::PtrEq,
    ];

    pub fn name(self) -> &'static str {
        use PrimOp::*;
        match self {
            Add => "add",
            Sub => "sub",
            Mul => "mul",
            Quot => "quot",
            Rem => "rem",
            Neg => "neg",
            And => "and",
            Or => "or",
            Xor => "xor",
            Shl => "shl",
            Shr => "shr",
            Eq => "eq",
            Ne => "ne",
            Lt => "lt",
            Le => "le",
            Gt => "gt",
            Ge => "ge",
            FAdd => "fadd",
            FSub => "fsub",
            FMul => "fmul",
            FDiv => "fdiv",
            FNeg => "fneg",
            FEq => "feq",
            FLt => "flt",
            FLe => "fle",
            IntToDouble => "int_to_double",
            DoubleToInt => "double_to_int",
            BytesNew => "bytes_new",
            BytesLen => "bytes_len",
            BytesGet => "bytes_get",
            BytesSet => "bytes_set",
            ArrayNew => "array_new",
            ArrayLen => "array_len",
            ArrayGet => "array_get",
            ArraySet => "array_set",
            ArrayUnique => "array_unique",
            BytesGet64 => "bytes_get64",
            BytesSet64 => "bytes_set64",
            BytesGetF => "bytes_getf",
            BytesSetF => "bytes_setf",
            BytesUnique => "bytes_unique",
            ArraySetShared => "array_set_shared",
            BytesSet64Shared => "bytes_set64_shared",
            BytesSetFShared => "bytes_setf_shared",
            PtrEq => "ptr_eq",
        }
    }

    pub fn from_name(s: &str) -> Option<PrimOp> {
        PrimOp::ALL.iter().copied().find(|p| p.name() == s)
    }

    /// Number of type arguments (`array_*` take the element type).
    pub fn ty_arity(self) -> usize {
        use PrimOp::*;
        match self {
            ArrayNew | ArrayLen | ArrayGet | ArraySet | ArrayUnique | ArraySetShared | PtrEq => 1,
            _ => 0,
        }
    }

    /// Argument and result types, given the type arguments.
    pub fn signature(self, targs: &[Type]) -> (Vec<Type>, Type) {
        use PrimOp::*;
        let i = Type::int;
        let d = Type::double;
        let elem = targs.first().cloned().unwrap_or(Type::Any);
        let arr = Type::Con(ARRAY.into(), vec![elem.clone()]);
        match self {
            Add | Sub | Mul | Quot | Rem | And | Or | Xor | Shl | Shr | Eq | Ne | Lt | Le | Gt
            | Ge => (vec![i(), i()], i()),
            Neg => (vec![i()], i()),
            FAdd | FSub | FMul | FDiv => (vec![d(), d()], d()),
            FNeg => (vec![d()], d()),
            FEq | FLt | FLe => (vec![d(), d()], i()),
            IntToDouble => (vec![i()], d()),
            DoubleToInt => (vec![d()], i()),
            BytesNew => (vec![i()], Type::bytes()),
            BytesLen => (vec![Type::bytes()], i()),
            BytesGet => (vec![Type::bytes(), i()], i()),
            BytesSet => (vec![Type::bytes(), i(), i()], Type::unit()),
            ArrayNew => (vec![i(), elem.clone()], arr),
            ArrayLen => (vec![arr], i()),
            ArrayGet => (vec![arr, i()], elem),
            ArraySet | ArraySetShared => (vec![arr, i(), elem], Type::unit()),
            ArrayUnique => (vec![arr.clone()], arr),
            BytesGet64 => (vec![Type::bytes(), i()], i()),
            BytesSet64 | BytesSet64Shared => (vec![Type::bytes(), i(), i()], Type::unit()),
            BytesGetF => (vec![Type::bytes(), i()], d()),
            BytesSetF | BytesSetFShared => (vec![Type::bytes(), i(), d()], Type::unit()),
            BytesUnique => (vec![Type::bytes()], Type::bytes()),
            PtrEq => (vec![elem.clone(), elem], i()),
        }
    }

    /// Operations with side effects or that may fail; never removed or
    /// duplicated by the optimizer.
    pub fn effectful(self) -> bool {
        use PrimOp::*;
        matches!(
            self,
            Quot | Rem
                | BytesNew
                | BytesSet
                | BytesGet
                | ArrayNew
                | ArrayGet
                | ArraySet
                | ArrayUnique
                | BytesGet64
                | BytesSet64
                | BytesGetF
                | BytesSetF
                | BytesUnique
                | ArraySetShared
                | BytesSet64Shared
                | BytesSetFShared
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AltCon {
    /// Data constructor with binders for its fields.
    Con(String, Vec<Binder>),
    /// Integer literal (scrutinee of type `Int#`).
    Int(i64),
    Default,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alt {
    pub con: AltCon,
    pub rhs: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Bind {
    NonRec(Binder, Box<Expr>),
    Rec(Vec<(Binder, Expr)>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// Local variable.
    Var(Id),
    /// Top-level binding.
    Global(String),
    Lit(Lit),
    /// Saturated constructor application with its type arguments.
    Con(String, Vec<Type>, Vec<Expr>),
    App(Box<Expr>, Box<Expr>),
    TyApp(Box<Expr>, Type),
    Lam(Binder, Box<Expr>),
    TyLam(TyVar, Kind, Box<Expr>),
    /// Lazy `let` (the right-hand side becomes a thunk unless it is a value).
    Let(Bind, Box<Expr>),
    /// Strict `let!`: evaluates the right-hand side (to WHNF) first.
    LetStrict(Binder, Box<Expr>, Box<Expr>),
    /// `case e as b : τ of alts` — evaluates `e` to WHNF, binds it to `b`.
    Case(Box<Expr>, Binder, Type, Vec<Alt>),
    /// Representation-preserving conversion (newtypes, `Any`).
    Cast(Box<Expr>, Type),
    Prim(PrimOp, Vec<Type>, Vec<Expr>),
    /// Call of a C function declared with `extern` (effectful, strict).
    Extern(String, Vec<Expr>),
    /// Runtime error (`error`, incomplete patterns): never returns.
    Panic(String, Type),
}

impl Expr {
    pub fn var(id: &Id) -> Expr {
        Expr::Var(id.clone())
    }

    pub fn app(f: Expr, args: impl IntoIterator<Item = Expr>) -> Expr {
        args.into_iter()
            .fold(f, |f, a| Expr::App(Box::new(f), Box::new(a)))
    }

    pub fn int(n: i64) -> Expr {
        Expr::Lit(Lit::Int(n))
    }

    /// Splits `f a b c` into `f` and `[a, b, c]` (type applications are
    /// kept in the head).
    pub fn collect_args(&self) -> (&Expr, Vec<&Expr>) {
        let mut args = Vec::new();
        let mut e = self;
        while let Expr::App(f, a) = e {
            args.push(&**a);
            e = f;
        }
        args.reverse();
        (e, args)
    }

    /// True for expressions that are already values (WHNF) and need no
    /// thunk when bound lazily.
    pub fn is_value(&self) -> bool {
        match self {
            Expr::Lit(_) | Expr::Lam(..) | Expr::Var(_) => true,
            Expr::TyLam(_, _, e) | Expr::Cast(e, _) => e.is_value(),
            Expr::Con(_, _, args) => args.iter().all(|a| a.is_atom()),
            Expr::Global(_) => true,
            _ => false,
        }
    }

    pub fn is_atom(&self) -> bool {
        match self {
            Expr::Var(_) | Expr::Lit(_) | Expr::Global(_) => true,
            Expr::TyApp(e, _) | Expr::Cast(e, _) => e.is_atom(),
            _ => false,
        }
    }
}

/// A data constructor declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct CtorDecl {
    pub name: String,
    /// Field types (may mention the data type's parameters).
    pub fields: Vec<Type>,
    /// Strict fields are evaluated before construction.
    pub strict: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataDecl {
    pub name: String,
    pub params: Vec<(TyVar, Kind)>,
    /// `LType` when declared linear-only (`type T :: LType`).
    pub linear: bool,
    pub ctors: Vec<CtorDecl>,
    /// A newtype is represented as its single field (erased by `cast`).
    pub newtype: bool,
}

/// A C function callable through `Expr::Extern`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<Type>,
    pub ret: Type,
    /// The C signature of a foreign function (FFI), when it differs from
    /// the runtime's convention (`Int#` = `int64_t`, `Double#` = `double`,
    /// `Bytes` = object pointer, `Unit#` result = `void`). The Core types
    /// are then `Int#` (integers and pointers), `Double#` (floating point)
    /// and `Unit#` (`void`); the backend converts at the call.
    pub c_sig: Option<CSig>,
}

/// A C signature: parameter and result types.
#[derive(Clone, Debug, PartialEq)]
pub struct CSig {
    pub params: Vec<CTy>,
    pub ret: CTy,
}

/// A C scalar type at the FFI boundary (x86_64 System V).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CTy {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    /// Any data or function pointer (`void *`).
    Ptr,
    Void,
}

impl CTy {
    pub const ALL: &'static [CTy] = &[
        CTy::I8,
        CTy::U8,
        CTy::I16,
        CTy::U16,
        CTy::I32,
        CTy::U32,
        CTy::I64,
        CTy::U64,
        CTy::F32,
        CTy::F64,
        CTy::Ptr,
        CTy::Void,
    ];

    pub fn name(self) -> &'static str {
        match self {
            CTy::I8 => "i8",
            CTy::U8 => "u8",
            CTy::I16 => "i16",
            CTy::U16 => "u16",
            CTy::I32 => "i32",
            CTy::U32 => "u32",
            CTy::I64 => "i64",
            CTy::U64 => "u64",
            CTy::F32 => "f32",
            CTy::F64 => "f64",
            CTy::Ptr => "ptr",
            CTy::Void => "void",
        }
    }

    pub fn from_name(s: &str) -> Option<CTy> {
        CTy::ALL.iter().copied().find(|t| t.name() == s)
    }

    pub fn is_float(self) -> bool {
        matches!(self, CTy::F32 | CTy::F64)
    }

    /// Bits of an integer type (`ptr` is 64).
    pub fn int_bits(self) -> Option<u32> {
        match self {
            CTy::I8 | CTy::U8 => Some(8),
            CTy::I16 | CTy::U16 => Some(16),
            CTy::I32 | CTy::U32 => Some(32),
            CTy::I64 | CTy::U64 | CTy::Ptr => Some(64),
            _ => None,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, CTy::I8 | CTy::I16 | CTy::I32 | CTy::I64)
    }
}

/// A top-level binding.
#[derive(Clone, Debug, PartialEq)]
pub struct TopBind {
    pub name: String,
    pub ty: Type,
    pub expr: Expr,
    pub span: Span,
}

/// A whole program. The entry point is the binding `main : Int#` (its
/// value is the exit code).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Program {
    pub datas: Vec<DataDecl>,
    pub externs: Vec<ExternDecl>,
    pub binds: Vec<TopBind>,
}

impl Program {
    pub fn data(&self, name: &str) -> Option<&DataDecl> {
        self.datas.iter().find(|d| d.name == name)
    }

    /// The data type declaring constructor `c`, and the constructor's index.
    pub fn ctor(&self, c: &str) -> Option<(&DataDecl, usize)> {
        self.datas
            .iter()
            .find_map(|d| d.ctors.iter().position(|k| k.name == c).map(|i| (d, i)))
    }

    pub fn bind(&self, name: &str) -> Option<&TopBind> {
        self.binds.iter().find(|b| b.name == name)
    }

    pub fn extern_decl(&self, name: &str) -> Option<&ExternDecl> {
        self.externs.iter().find(|e| e.name == name)
    }

    /// Largest variable number used (for generating fresh ids).
    pub fn max_id(&self) -> u32 {
        fn binder(b: &Binder, m: &mut u32) {
            *m = (*m).max(b.id.id);
        }
        fn go(e: &Expr, m: &mut u32) {
            match e {
                Expr::Var(i) => *m = (*m).max(i.id),
                Expr::Global(_) | Expr::Lit(_) | Expr::Panic(..) => {}
                Expr::Con(_, _, a) | Expr::Prim(_, _, a) | Expr::Extern(_, a) => {
                    a.iter().for_each(|x| go(x, m))
                }
                Expr::App(f, a) => {
                    go(f, m);
                    go(a, m);
                }
                Expr::TyApp(e, _) | Expr::TyLam(_, _, e) | Expr::Cast(e, _) => go(e, m),
                Expr::Lam(b, e) => {
                    binder(b, m);
                    go(e, m);
                }
                Expr::Let(Bind::NonRec(b, r), e) => {
                    binder(b, m);
                    go(r, m);
                    go(e, m);
                }
                Expr::Let(Bind::Rec(bs), e) => {
                    for (b, r) in bs {
                        binder(b, m);
                        go(r, m);
                    }
                    go(e, m);
                }
                Expr::LetStrict(b, r, e) => {
                    binder(b, m);
                    go(r, m);
                    go(e, m);
                }
                Expr::Case(s, b, _, alts) => {
                    go(s, m);
                    binder(b, m);
                    for a in alts {
                        if let AltCon::Con(_, bs) = &a.con {
                            bs.iter().for_each(|b| binder(b, m));
                        }
                        go(&a.rhs, m);
                    }
                }
            }
        }
        let mut m = 0;
        for b in &self.binds {
            go(&b.expr, &mut m);
        }
        m
    }
}
