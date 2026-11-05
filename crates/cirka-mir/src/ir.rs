//! MIR data structures (06-ir.md §2).
//!
//! MIR is first-order, in A-normal form, strict, and makes ownership
//! explicit. Every function is top level; variables are function-local and
//! defined exactly once (block parameters play the role of phi nodes).

use cirka_diagnostics::Span;
use std::collections::HashMap;
use std::fmt;

/// Runtime representation of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// A pointer to a boxed object (RC managed unless static).
    Ptr,
    I64,
    I32,
    I8,
    F64,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::Ptr => "ptr",
            Ty::I64 => "i64",
            Ty::I32 => "i32",
            Ty::I8 => "i8",
            Ty::F64 => "f64",
        }
    }

    pub fn from_name(s: &str) -> Option<Ty> {
        Some(match s {
            "ptr" => Ty::Ptr,
            "i64" => Ty::I64,
            "i32" => Ty::I32,
            "i8" => Ty::I8,
            "f64" => Ty::F64,
            _ => return None,
        })
    }

    pub fn is_ptr(self) -> bool {
        self == Ty::Ptr
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Ownership of a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Own {
    Owned,
    Borrowed,
}

/// A function-local variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Var(pub u32);

/// A basic block index within a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// A constant operand.
#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Int(i64, Ty),
    Float(f64),
    /// A static immortal `BYTES` object.
    Str(Vec<u8>),
}

impl Const {
    pub fn ty(&self) -> Ty {
        match self {
            Const::Int(_, t) => *t,
            Const::Float(_) => Ty::F64,
            Const::Str(_) => Ty::Ptr,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Var(Var),
    Const(Const),
}

impl Operand {
    pub fn var(&self) -> Option<Var> {
        match self {
            Operand::Var(v) => Some(*v),
            Operand::Const(_) => None,
        }
    }
}

/// Reference to a constructor `Type.Ctor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CtorRef {
    pub ty: u32,
    pub ctor: u32,
}

/// Right-hand side of a `let`.
#[derive(Clone, Debug, PartialEq)]
pub enum Rhs {
    Lit(Const),
    Prim(Prim, Vec<Operand>),
    Alloc {
        ctor: CtorRef,
        args: Vec<Operand>,
        reuse: Option<Var>,
    },
    Proj {
        obj: Var,
        ctor: CtorRef,
        field: u32,
    },
    Call {
        func: String,
        args: Vec<Operand>,
    },
    Apply {
        clo: Var,
        args: Vec<Operand>,
    },
    MkClosure {
        func: String,
        args: Vec<Operand>,
    },
    MkThunk {
        func: String,
        args: Vec<Operand>,
    },
    Force(Var),
    DropReuse(Var),
    Extern {
        func: String,
        args: Vec<Operand>,
    },
    /// Address of a global (static immortal thunk / CAF).
    Global(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Instr {
    /// `let x = rhs`; `var` is `None` for `let _ = extern f(..)` returning void.
    Let {
        var: Option<Var>,
        rhs: Rhs,
    },
    Dup(Var),
    Drop(Var),
    /// Destructive update `set obj T.C.field = val` (owned unique objects only).
    Set {
        obj: Var,
        ctor: CtorRef,
        field: u32,
        val: Operand,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Case {
    Ctor(CtorRef),
    Int(i64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Terminator {
    Ret(Operand),
    Jump(BlockId, Vec<Operand>),
    Switch {
        scrut: Var,
        arms: Vec<(Case, BlockId)>,
        default: Option<BlockId>,
    },
    TailCall {
        func: String,
        args: Vec<Operand>,
    },
    Panic(String),
    Unreachable,
}

impl Terminator {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(b, _) => vec![*b],
            Terminator::Switch { arms, default, .. } => {
                let mut v: Vec<BlockId> = arms.iter().map(|a| a.1).collect();
                v.extend(default.iter().copied());
                v
            }
            _ => vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub label: String,
    pub params: Vec<Var>,
    pub instrs: Vec<Instr>,
    pub term: Terminator,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub var: Var,
    pub own: Own,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Ty,
    pub blocks: Vec<Block>,
    /// Names of variables (index = `Var.0`).
    pub var_names: Vec<String>,
    /// Types of variables (index = `Var.0`).
    pub var_tys: Vec<Ty>,
    pub span: Span,
}

impl Function {
    pub fn new(name: impl Into<String>, ret: Ty) -> Function {
        Function {
            name: name.into(),
            params: Vec::new(),
            ret,
            blocks: Vec::new(),
            var_names: Vec::new(),
            var_tys: Vec::new(),
            span: Span::DUMMY,
        }
    }

    pub fn new_var(&mut self, name: impl Into<String>, ty: Ty) -> Var {
        self.var_names.push(name.into());
        self.var_tys.push(ty);
        Var(self.var_names.len() as u32 - 1)
    }

    /// A fresh variable whose printed name is unique within the function.
    pub fn fresh_var(&mut self, hint: &str, ty: Ty) -> Var {
        let n = self.var_names.len();
        self.new_var(format!("{hint}%{n}"), ty)
    }

    pub fn ty(&self, v: Var) -> Ty {
        self.var_tys[v.0 as usize]
    }

    pub fn var_name(&self, v: Var) -> &str {
        &self.var_names[v.0 as usize]
    }

    pub fn block(&self, b: BlockId) -> &Block {
        &self.blocks[b.0 as usize]
    }

    pub fn param_tys(&self) -> Vec<Ty> {
        self.params.iter().map(|p| self.ty(p.var)).collect()
    }

    /// Predecessor lists for every block.
    pub fn predecessors(&self) -> Vec<Vec<BlockId>> {
        let mut preds = vec![Vec::new(); self.blocks.len()];
        for (i, b) in self.blocks.iter().enumerate() {
            for s in b.term.successors() {
                if let Some(p) = preds.get_mut(s.0 as usize)
                    && !p.contains(&BlockId(i as u32))
                {
                    p.push(BlockId(i as u32));
                }
            }
        }
        preds
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CtorDef {
    pub name: String,
    pub tag: u32,
    pub fields: Vec<Field>,
}

impl CtorDef {
    /// Number of pointer fields (stored first).
    pub fn nptr(&self) -> usize {
        self.fields.iter().filter(|f| f.ty.is_ptr()).count()
    }

    /// Slot index (in 8-byte words after the header) of field `i`:
    /// pointer fields come first, then scalars, each in declaration order.
    pub fn slot(&self, i: usize) -> usize {
        let f = &self.fields[i];
        if f.ty.is_ptr() {
            self.fields[..i].iter().filter(|g| g.ty.is_ptr()).count()
        } else {
            self.nptr() + self.fields[..i].iter().filter(|g| !g.ty.is_ptr()).count()
        }
    }

    /// Object size in bytes (8-byte header + 8 bytes per field).
    pub fn size(&self) -> usize {
        8 + 8 * self.fields.len()
    }

    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.name == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeDef {
    pub name: String,
    pub ctors: Vec<CtorDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternDecl {
    pub name: String,
    pub params: Vec<(Ty, Own)>,
    /// `None` = `void`.
    pub ret: Option<Ty>,
    /// The C signature of a foreign function: the parameters are `i64`
    /// (integers, pointers) or `f64`, and the backend converts them to
    /// these C types at the call (and the result back).
    pub c_sig: Option<cirka_core::CSig>,
}

/// A static immortal thunk evaluated on first use (a CAF).
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalDecl {
    pub name: String,
    /// A function with no parameters that computes the value.
    pub init: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Module {
    pub types: Vec<TypeDef>,
    pub externs: Vec<ExternDecl>,
    pub globals: Vec<GlobalDecl>,
    pub funcs: Vec<Function>,
}

impl Module {
    pub fn ctor(&self, c: CtorRef) -> &CtorDef {
        &self.types[c.ty as usize].ctors[c.ctor as usize]
    }

    pub fn ctor_name(&self, c: CtorRef) -> String {
        let t = &self.types[c.ty as usize];
        format!("{}.{}", t.name, t.ctors[c.ctor as usize].name)
    }

    pub fn find_ctor(&self, ty: &str, ctor: &str) -> Option<CtorRef> {
        let ti = self.types.iter().position(|t| t.name == ty)?;
        let ci = self.types[ti].ctors.iter().position(|c| c.name == ctor)?;
        Some(CtorRef {
            ty: ti as u32,
            ctor: ci as u32,
        })
    }

    pub fn func(&self, name: &str) -> Option<&Function> {
        self.funcs.iter().find(|f| f.name == name)
    }

    pub fn func_index(&self) -> HashMap<&str, usize> {
        self.funcs
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.as_str(), i))
            .collect()
    }

    pub fn extern_decl(&self, name: &str) -> Option<&ExternDecl> {
        self.externs.iter().find(|e| e.name == name)
    }
}

/// Primitive operations. Primitives never consume their pointer arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    // i64 arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Neg,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    LShr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // f64
    FAdd,
    FSub,
    FMul,
    FDiv,
    FNeg,
    FEq,
    FNe,
    FLt,
    FLe,
    FGt,
    FGe,
    // conversions
    I64ToF64,
    F64ToI64,
    I64ToI32,
    I32ToI64,
    I64ToI8,
    I8ToI64,
    F64Bits,
    BitsF64,
    // objects
    PtrEq,
    IsUnique,
    BytesNew,
    BytesLen,
    BytesGet,
    BytesSet,
    BytesData,
    ArrayNew,
    ArrayLen,
    ArrayGet,
    ArraySet,
    ArrayUnique,
    BytesGetI64,
    BytesSetI64,
    BytesGetF64,
    BytesSetF64,
    BytesUnique,
    ArraySetShared,
    BytesSetI64Shared,
    BytesSetF64Shared,
}

impl Prim {
    pub const ALL: &'static [Prim] = &[
        Prim::Add,
        Prim::Sub,
        Prim::Mul,
        Prim::Div,
        Prim::Rem,
        Prim::Neg,
        Prim::And,
        Prim::Or,
        Prim::Xor,
        Prim::Shl,
        Prim::Shr,
        Prim::LShr,
        Prim::Eq,
        Prim::Ne,
        Prim::Lt,
        Prim::Le,
        Prim::Gt,
        Prim::Ge,
        Prim::FAdd,
        Prim::FSub,
        Prim::FMul,
        Prim::FDiv,
        Prim::FNeg,
        Prim::FEq,
        Prim::FNe,
        Prim::FLt,
        Prim::FLe,
        Prim::FGt,
        Prim::FGe,
        Prim::I64ToF64,
        Prim::F64ToI64,
        Prim::I64ToI32,
        Prim::I32ToI64,
        Prim::I64ToI8,
        Prim::I8ToI64,
        Prim::F64Bits,
        Prim::BitsF64,
        Prim::PtrEq,
        Prim::IsUnique,
        Prim::BytesNew,
        Prim::BytesLen,
        Prim::BytesGet,
        Prim::BytesSet,
        Prim::BytesData,
        Prim::ArrayNew,
        Prim::ArrayLen,
        Prim::ArrayGet,
        Prim::ArraySet,
        Prim::ArrayUnique,
        Prim::BytesGetI64,
        Prim::BytesSetI64,
        Prim::BytesGetF64,
        Prim::BytesSetF64,
        Prim::BytesUnique,
        Prim::ArraySetShared,
        Prim::BytesSetI64Shared,
        Prim::BytesSetF64Shared,
    ];

    pub fn name(self) -> &'static str {
        use Prim::*;
        match self {
            Add => "add",
            Sub => "sub",
            Mul => "mul",
            Div => "div",
            Rem => "rem",
            Neg => "neg",
            And => "and",
            Or => "or",
            Xor => "xor",
            Shl => "shl",
            Shr => "shr",
            LShr => "lshr",
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
            FNe => "fne",
            FLt => "flt",
            FLe => "fle",
            FGt => "fgt",
            FGe => "fge",
            I64ToF64 => "i64_to_f64",
            F64ToI64 => "f64_to_i64",
            I64ToI32 => "i64_to_i32",
            I32ToI64 => "i32_to_i64",
            I64ToI8 => "i64_to_i8",
            I8ToI64 => "i8_to_i64",
            F64Bits => "f64_bits",
            BitsF64 => "bits_f64",
            PtrEq => "ptr_eq",
            IsUnique => "is_unique",
            BytesNew => "bytes_new",
            BytesLen => "bytes_len",
            BytesGet => "bytes_get",
            BytesSet => "bytes_set",
            BytesData => "bytes_data",
            ArrayNew => "array_new",
            ArrayLen => "array_len",
            ArrayGet => "array_get",
            ArraySet => "array_set",
            ArrayUnique => "array_unique",
            BytesGetI64 => "bytes_get_i64",
            BytesSetI64 => "bytes_set_i64",
            BytesGetF64 => "bytes_get_f64",
            BytesSetF64 => "bytes_set_f64",
            BytesUnique => "bytes_unique",
            ArraySetShared => "array_set_shared",
            BytesSetI64Shared => "bytes_set_i64_shared",
            BytesSetF64Shared => "bytes_set_f64_shared",
        }
    }

    pub fn from_name(s: &str) -> Option<Prim> {
        Prim::ALL.iter().copied().find(|p| p.name() == s)
    }

    /// Parameter types and result type.
    pub fn signature(self) -> (&'static [Ty], Ty) {
        use Prim::*;
        use Ty::*;
        match self {
            Add | Sub | Mul | Div | Rem | And | Or | Xor | Shl | Shr | LShr | Eq | Ne | Lt | Le
            | Gt | Ge => (&[I64, I64], I64),
            Neg => (&[I64], I64),
            FAdd | FSub | FMul | FDiv => (&[F64, F64], F64),
            FNeg => (&[F64], F64),
            FEq | FNe | FLt | FLe | FGt | FGe => (&[F64, F64], I64),
            I64ToF64 => (&[I64], F64),
            F64ToI64 => (&[F64], I64),
            I64ToI32 => (&[I64], I32),
            I32ToI64 => (&[I32], I64),
            I64ToI8 => (&[I64], I8),
            I8ToI64 => (&[I8], I64),
            F64Bits => (&[F64], I64),
            BitsF64 => (&[I64], F64),
            PtrEq => (&[Ptr, Ptr], I64),
            IsUnique => (&[Ptr], I64),
            // bytes_new(len): a fresh zeroed BYTES object (owned).
            BytesNew => (&[I64], Ptr),
            BytesLen => (&[Ptr], I64),
            BytesGet => (&[Ptr, I64], I64),
            // bytes_set(b, i, v): in-place store; `b` must be unique.
            BytesSet => (&[Ptr, I64, I64], I64),
            // Raw pointer to the first byte (for C FFI); not an object.
            BytesData => (&[Ptr], I64),
            // array_new(len, init): every element is `init` (dup'ed len times).
            ArrayNew => (&[I64, Ptr], Ptr),
            ArrayLen => (&[Ptr], I64),
            // array_get returns a borrowed element (dup it to keep it).
            ArrayGet => (&[Ptr, I64], Ptr),
            // array_set(a, i, v): consumes `v`, drops the old element; `a` must be unique.
            ArraySet => (&[Ptr, I64, Ptr], I64),
            // array_unique(a): consumes `a`; returns it when unique, otherwise
            // a fresh copy (elements dup'ed) and `a` is dropped.
            ArrayUnique => (&[Ptr], Ptr),
            // 8-byte words (element `i` at byte offset 8*i) of a BYTES
            // object: the representation of unboxed arrays (FEAT-0123).
            BytesGetI64 => (&[Ptr, I64], I64),
            // bytes_set_i64(b, i, v): in-place store; `b` must be unique.
            BytesSetI64 => (&[Ptr, I64, I64], I64),
            BytesGetF64 => (&[Ptr, I64], F64),
            BytesSetF64 => (&[Ptr, I64, F64], I64),
            // bytes_unique(b): like array_unique for BYTES.
            BytesUnique => (&[Ptr], Ptr),
            // Stores through a mutable borrow (FEAT-0124): the object may be
            // shared with its borrowed owner; otherwise like the plain stores.
            ArraySetShared => (&[Ptr, I64, Ptr], I64),
            BytesSetI64Shared => (&[Ptr, I64, I64], I64),
            BytesSetF64Shared => (&[Ptr, I64, F64], I64),
        }
    }

    /// Whether the result is a borrowed pointer into an argument (like `proj`).
    pub fn result_borrowed(self) -> bool {
        matches!(self, Prim::ArrayGet)
    }

    /// Indices of pointer arguments that are consumed.
    pub fn consumes(self) -> &'static [usize] {
        match self {
            Prim::ArrayNew => &[1],
            Prim::ArraySet | Prim::ArraySetShared => &[2],
            Prim::ArrayUnique | Prim::BytesUnique => &[0],
            _ => &[],
        }
    }
}

/// Kinds of heap objects (05-runtime.md §1). Must match `cirka-runtime`.
pub mod kind {
    pub const CON: u8 = 0;
    pub const FUN: u8 = 1;
    pub const PAP: u8 = 2;
    pub const THUNK: u8 = 3;
    pub const BLACKHOLE: u8 = 4;
    pub const IND: u8 = 5;
    pub const ARRAY: u8 = 6;
    pub const BYTES: u8 = 7;
    pub const BIGINT: u8 = 8;
    pub const FOREIGN: u8 = 9;
    pub const MUTVAR: u8 = 10;
    pub const MVAR: u8 = 11;
}

/// Maximum arity of a function called through a closure (`apply`).
pub const MAX_CLOSURE_ARITY: usize = 8;
