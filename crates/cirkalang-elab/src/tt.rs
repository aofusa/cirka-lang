//! TT: the dependently typed core language of CirkaLang (spec-v0.0.1.md
//! §4.1), its values (for normalization by evaluation) and the elaborated
//! program.

use cirka_diagnostics::Span;
pub use cirkalang_resolve::rast::Abi;
pub use cirkalang_resolve::rast::GName;
pub use cirkalang_resolve::rast::Totality;
use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;

pub type Name = Rc<str>;
pub type MetaId = u32;

/// Multiplicities {0, 1, ω} (spec §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mult {
    Zero,
    One,
    Many,
}

impl Mult {
    pub fn plus(self, o: Mult) -> Mult {
        match (self, o) {
            (Mult::Zero, x) | (x, Mult::Zero) => x,
            _ => Mult::Many,
        }
    }

    pub fn times(self, o: Mult) -> Mult {
        match (self, o) {
            (Mult::Zero, _) | (_, Mult::Zero) => Mult::Zero,
            (Mult::One, x) | (x, Mult::One) => x,
            _ => Mult::Many,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mult::Zero => "0",
            Mult::One => "1",
            Mult::Many => "ω",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icit {
    Expl,
    Impl,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    UInt(u64),
    Float(f64),
    Char(char),
    Str(Rc<str>),
}

impl std::fmt::Display for Lit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lit::Int(n) => write!(f, "{n}"),
            Lit::UInt(n) => write!(f, "{n}u"),
            Lit::Float(x) => write!(f, "{x:?}"),
            Lit::Char(c) => write!(f, "{c:?}"),
            Lit::Str(s) => write!(f, "{s:?}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    /// De Bruijn index.
    Var(u32),
    /// A definition, constructor, data type or primitive.
    Global(GName),
    Type,
    LType,
    Pi(Name, Mult, Icit, Rc<Term>, Rc<Term>),
    /// `\(π x : A) => t` (the binder type is kept for lowering and Lint).
    Lam(Name, Mult, Icit, Rc<Term>, Rc<Term>),
    App(Rc<Term>, Rc<Term>, Icit),
    /// Strict `let π x : A = t in u`.
    Let(Name, Mult, Rc<Term>, Rc<Term>, Rc<Term>),
    Lit(Lit),
    Case(Rc<Case>),
    Meta(MetaId),
    /// A meta applied to the bound variables of the context (mask by level).
    InsertedMeta(MetaId, Rc<Vec<bool>>),
    /// The result of an error that has been reported.
    Error,
}

/// `case scrut return ret of alts` (spec §4.5). Elaborated programs always
/// scrutinize a variable; quoted stuck cases may have any neutral.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub scrut: Term,
    /// Type of the whole expression (in the context of the case).
    pub ret: Term,
    pub alts: Vec<Alt>,
    pub default: Option<Term>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Alt {
    Con {
        con: GName,
        /// All fields (implicit and erased ones included), with their
        /// multiplicity and type (each in the context of the previous ones).
        fields: Vec<(Name, Mult, Term)>,
        /// Index refinements learnt by dependent pattern matching:
        /// variable (de Bruijn index in the body's context) := term.
        refine: Vec<(u32, Term)>,
        body: Term,
    },
    Lit {
        lit: Lit,
        body: Term,
    },
}

// ---------------------------------------------------------------------
// Values

pub type Env = Vec<Val>;

#[derive(Clone, Debug)]
pub struct Closure {
    pub env: Env,
    pub body: Rc<Term>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Head {
    /// Bound variable (de Bruijn level).
    Var(u32),
    Meta(MetaId),
    /// Definition or primitive (unfolded on demand).
    Global(GName),
}

#[derive(Clone, Debug)]
pub enum Elim {
    App(Val, Icit),
    /// A case stuck on the neutral (branches evaluated in `env`).
    Case(Env, Rc<Case>),
}

#[derive(Clone, Debug)]
pub enum Val {
    Neutral(Head, Vec<Elim>),
    /// Constructor applied to (some of) its arguments: parameters, then
    /// fields.
    Con(GName, Vec<Val>),
    /// Data type applied to (some of) its parameters and indices.
    Data(GName, Vec<(Val, Icit)>),
    Lam(Name, Mult, Icit, Rc<Val>, Closure),
    Pi(Name, Mult, Icit, Rc<Val>, Closure),
    Type,
    LType,
    Lit(Lit),
    Error,
}

impl Val {
    pub fn var(l: u32) -> Val {
        Val::Neutral(Head::Var(l), Vec::new())
    }
}

// ---------------------------------------------------------------------
// The elaborated program

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Univ {
    Type,
    LType,
}

#[derive(Clone, Debug)]
pub enum Global {
    /// Not elaborated (because of an earlier error).
    Missing,
    Def {
        ty: Term,
        /// `None` while being elaborated (opaque) or after an error.
        body: Option<Term>,
    },
    Data {
        ty: Term,
        nparams: usize,
        univ: Univ,
        cons: Vec<GName>,
        /// Parameters of sort `LType` whose linearity the type propagates:
        /// `univ` is the universe when they are all instantiated with
        /// shareable types, and the type is linear when one of them is
        /// (spec-v0.0.2.md §5.1).
        lin_params: Vec<bool>,
    },
    Con {
        ty: Term,
        data: GName,
        nparams: usize,
        /// Field multiplicities (after the parameters).
        field_mults: Vec<Mult>,
        /// Field plicities (implicit fields are not written in patterns).
        field_icits: Vec<Icit>,
        /// Fields declared `ω` (spec-v0.0.2.md §5.6): unrestricted even in
        /// a linear value. Other fields are used as often as the value.
        field_shared: Vec<bool>,
        tag: usize,
    },
    Prim {
        ty: Term,
        prim: Prim,
    },
    /// A function of an `extern` block (spec-v0.0.2.md §8), called by its
    /// source name.
    Extern {
        ty: Term,
        abi: Abi,
    },
}

/// The primitives known to the compiler (spec §5.4, §5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    // Types.
    Int,
    UInt,
    Float,
    Char,
    String,
    IO,
    IO1,
    Unique,
    // Int.
    AddInt,
    SubInt,
    MulInt,
    DivInt,
    ModInt,
    NegInt,
    EqInt,
    LtInt,
    LeInt,
    AndInt,
    OrInt,
    XorInt,
    ShlInt,
    ShrInt,
    // UInt.
    AddUInt,
    SubUInt,
    MulUInt,
    EqUInt,
    LtUInt,
    LeUInt,
    IntToUInt,
    UIntToInt,
    // Float.
    AddFloat,
    SubFloat,
    MulFloat,
    DivFloat,
    NegFloat,
    EqFloat,
    LtFloat,
    LeFloat,
    IntToFloat,
    FloatToInt,
    SqrtFloat,
    TruncFloat,
    // Char.
    CharToInt,
    IntToChar,
    // String.
    StrLength,
    StrIndex,
    StrConcat,
    StrEq,
    // Byte-level string operations (spec-v0.0.2.md §7.4).
    StrSlice,
    StrFindByte,
    StrFind,
    StrRangeEq,
    StrNew,
    StrWrite,
    StrSetByte,
    IntToStr,
    FloatToStr,
    CharToStr,
    // IO.
    Pure,
    Bind,
    PutStr,
    PrintInt,
    PrintFloat,
    PutChar,
    ReadInt,
    ArgCount,
    GetArg,
    Exit,
    Panic,
    /// `primRewrite {a} p x y eq v : p x` for `v : p y` (spec-v0.0.4.md §2.2).
    Rewrite,
    /// `primRefine {a} p site x = x`: marks a value checked against a
    /// refinement type (spec-v0.0.4.md §3).
    Refine,
    /// `subsetOf a p = a`: the refinement type `{x : a | p x}`.
    Subset,
    /// Operations whose check a proof makes unnecessary (spec-v0.0.4.md
    /// §5): used only by library functions whose arguments are refined.
    ArrayIndexProved,
    DivProved,
    ModProved,
    Unreachable,
    // Unique.
    MkUnique,
    Freeze,
    // Resources: giving up a value that holds only memory.
    DropFree,
    ReleaseFree,
    // Arrays (spec-v0.0.2.md §7): boxed `Array a` and unboxed `UArray a`
    // (8-byte words). Updates of a unique array are in place.
    Array,
    UArray,
    ArrayNew,
    ArrayEmpty,
    ArraySize,
    ArrayIndex,
    ArrayRead,
    ArrayLength,
    ArrayWrite,
    ArrayThaw,
    UArrayNew,
    UArraySize,
    UArrayLength,
    UArrayThaw,
    UArrayIndexWord,
    UArrayReadWord,
    UArrayWriteWord,
    UArrayIndexFloat,
    UArrayReadFloat,
    UArrayWriteFloat,
    // Borrows (spec-v0.0.3.md §2): the types and the wrappers of their
    // representations.
    Region,
    Ref,
    Borrow,
    BorrowIO,
    RegionVal,
    RefVal,
    BorrowWrap,
    BorrowRun,
    BorrowIOWrap,
    BorrowIORun,
    // Regions (spec-v0.0.3.md §3): an arena of cells addressed by `Int`.
    Arena,
    RIO,
    In,
    RefIn,
    RegionNew,
    RegionFree,
    RegionCell,
    RegionGet,
    RegionSet,
    RIOWrap,
    RIORun,
    InWrap,
    InCell,
    RefInWrap,
    RefInCell,
    // Explicit laziness (spec-v0.0.3.md §4).
    Lazy,
    Delay,
    Force,
    // Threads (spec-v0.0.5.md §1): the mark of values bound to their
    // thread, and the runtime's thread handles, `MVar`s and channels.
    ThreadBound,
    ThreadBoundVal,
    Thread,
    MVar,
    Chan,
    ThreadSpawn,
    ThreadJoin,
    MVarNew,
    MVarNewEmpty,
    MVarTake,
    MVarRead,
    MVarPut,
    MVarTryTake,
    MVarTryPut,
    ChanNew,
    ChanSend,
    ChanRecv,
    ChanTryRecv,
    ChanLength,
}

impl Prim {
    pub const ALL: &'static [(&'static str, Prim)] = &[
        ("Int", Prim::Int),
        ("UInt", Prim::UInt),
        ("Float", Prim::Float),
        ("Char", Prim::Char),
        ("String", Prim::String),
        ("IO", Prim::IO),
        ("IO1", Prim::IO1),
        ("Unique", Prim::Unique),
        ("primAddInt", Prim::AddInt),
        ("primSubInt", Prim::SubInt),
        ("primMulInt", Prim::MulInt),
        ("primDivInt", Prim::DivInt),
        ("primModInt", Prim::ModInt),
        ("primNegInt", Prim::NegInt),
        ("primEqInt", Prim::EqInt),
        ("primLtInt", Prim::LtInt),
        ("primLeInt", Prim::LeInt),
        ("primAndInt", Prim::AndInt),
        ("primOrInt", Prim::OrInt),
        ("primXorInt", Prim::XorInt),
        ("primShlInt", Prim::ShlInt),
        ("primShrInt", Prim::ShrInt),
        ("primAddUInt", Prim::AddUInt),
        ("primSubUInt", Prim::SubUInt),
        ("primMulUInt", Prim::MulUInt),
        ("primEqUInt", Prim::EqUInt),
        ("primLtUInt", Prim::LtUInt),
        ("primLeUInt", Prim::LeUInt),
        ("primIntToUInt", Prim::IntToUInt),
        ("primUIntToInt", Prim::UIntToInt),
        ("primAddFloat", Prim::AddFloat),
        ("primSubFloat", Prim::SubFloat),
        ("primMulFloat", Prim::MulFloat),
        ("primDivFloat", Prim::DivFloat),
        ("primNegFloat", Prim::NegFloat),
        ("primEqFloat", Prim::EqFloat),
        ("primLtFloat", Prim::LtFloat),
        ("primLeFloat", Prim::LeFloat),
        ("primIntToFloat", Prim::IntToFloat),
        ("primFloatToInt", Prim::FloatToInt),
        ("primSqrtFloat", Prim::SqrtFloat),
        ("primTruncFloat", Prim::TruncFloat),
        ("primCharToInt", Prim::CharToInt),
        ("primIntToChar", Prim::IntToChar),
        ("primStrLength", Prim::StrLength),
        ("primStrIndex", Prim::StrIndex),
        ("primStrConcat", Prim::StrConcat),
        ("primStrEq", Prim::StrEq),
        ("primStrSlice", Prim::StrSlice),
        ("primStrFindByte", Prim::StrFindByte),
        ("primStrFind", Prim::StrFind),
        ("primStrRangeEq", Prim::StrRangeEq),
        ("primStrNew", Prim::StrNew),
        ("primStrWrite", Prim::StrWrite),
        ("primStrSetByte", Prim::StrSetByte),
        ("primIntToStr", Prim::IntToStr),
        ("primFloatToStr", Prim::FloatToStr),
        ("primCharToStr", Prim::CharToStr),
        ("primPure", Prim::Pure),
        ("primBind", Prim::Bind),
        // The same operations at other types (spec-v0.0.2.md §5.2).
        ("primPure1", Prim::Pure),
        ("primBind1", Prim::Bind),
        ("primBindTo1", Prim::Bind),
        ("primBind11", Prim::Bind),
        ("putStr", Prim::PutStr),
        ("printInt", Prim::PrintInt),
        ("printFloat", Prim::PrintFloat),
        ("putChar", Prim::PutChar),
        ("readInt", Prim::ReadInt),
        ("argCount", Prim::ArgCount),
        ("getArg", Prim::GetArg),
        ("exit", Prim::Exit),
        ("panic", Prim::Panic),
        ("primRewrite", Prim::Rewrite),
        ("primRefine", Prim::Refine),
        ("subsetOf", Prim::Subset),
        ("primArrayIndexProved", Prim::ArrayIndexProved),
        ("primDivProved", Prim::DivProved),
        ("primModProved", Prim::ModProved),
        ("primUnreachable", Prim::Unreachable),
        ("unique", Prim::MkUnique),
        ("freeze", Prim::Freeze),
        ("dropFree", Prim::DropFree),
        ("releaseFree", Prim::ReleaseFree),
        ("Array", Prim::Array),
        ("UArray", Prim::UArray),
        ("primArrayNew", Prim::ArrayNew),
        ("primArrayEmpty", Prim::ArrayEmpty),
        ("primArraySize", Prim::ArraySize),
        ("primArrayIndex", Prim::ArrayIndex),
        ("primArrayRead", Prim::ArrayRead),
        ("primArrayLength", Prim::ArrayLength),
        ("primArrayWrite", Prim::ArrayWrite),
        ("primArrayThaw", Prim::ArrayThaw),
        ("primUArrayNew", Prim::UArrayNew),
        ("primUArraySize", Prim::UArraySize),
        ("primUArrayLength", Prim::UArrayLength),
        ("primUArrayThaw", Prim::UArrayThaw),
        ("primUArrayIndexWord", Prim::UArrayIndexWord),
        ("primUArrayReadWord", Prim::UArrayReadWord),
        ("primUArrayWriteWord", Prim::UArrayWriteWord),
        ("primUArrayIndexFloat", Prim::UArrayIndexFloat),
        ("primUArrayReadFloat", Prim::UArrayReadFloat),
        ("primUArrayWriteFloat", Prim::UArrayWriteFloat),
        ("Region", Prim::Region),
        ("Ref", Prim::Ref),
        ("Borrow", Prim::Borrow),
        ("BorrowIO", Prim::BorrowIO),
        ("primRegion", Prim::RegionVal),
        ("primRef", Prim::RefVal),
        ("primBorrowWrap", Prim::BorrowWrap),
        ("primBorrowRun", Prim::BorrowRun),
        ("primBorrowIOWrap", Prim::BorrowIOWrap),
        ("primBorrowIORun", Prim::BorrowIORun),
        ("Arena", Prim::Arena),
        ("RIO", Prim::RIO),
        ("In", Prim::In),
        ("RefIn", Prim::RefIn),
        ("primRegionNew", Prim::RegionNew),
        ("primRegionFree", Prim::RegionFree),
        ("primRegionCell", Prim::RegionCell),
        ("primRegionGet", Prim::RegionGet),
        ("primRegionSet", Prim::RegionSet),
        ("primRIO", Prim::RIOWrap),
        ("primRunRIO", Prim::RIORun),
        ("primIn", Prim::InWrap),
        ("primInCell", Prim::InCell),
        ("primRefIn", Prim::RefInWrap),
        ("primRefCell", Prim::RefInCell),
        ("Lazy", Prim::Lazy),
        ("primDelay", Prim::Delay),
        ("primForce", Prim::Force),
        ("ThreadBound", Prim::ThreadBound),
        ("threadBound", Prim::ThreadBoundVal),
        ("Thread", Prim::Thread),
        ("MVar", Prim::MVar),
        ("Chan", Prim::Chan),
        ("primSpawn", Prim::ThreadSpawn),
        ("primJoin", Prim::ThreadJoin),
        ("primNewMVar", Prim::MVarNew),
        ("primNewEmptyMVar", Prim::MVarNewEmpty),
        ("primTakeMVar", Prim::MVarTake),
        ("primReadMVar", Prim::MVarRead),
        ("primPutMVar", Prim::MVarPut),
        ("primTryTakeMVar", Prim::MVarTryTake),
        ("primTryPutMVar", Prim::MVarTryPut),
        ("primNewChan", Prim::ChanNew),
        ("primSend", Prim::ChanSend),
        ("primRecv", Prim::ChanRecv),
        ("primTryRecv", Prim::ChanTryRecv),
        ("primChanLength", Prim::ChanLength),
    ];

    pub fn from_name(s: &str) -> Option<Prim> {
        Prim::ALL.iter().find(|(n, _)| *n == s).map(|x| x.1)
    }

    pub fn name(self) -> &'static str {
        Prim::ALL
            .iter()
            .find(|(_, p)| *p == self)
            .map(|x| x.0)
            .unwrap()
    }

    /// Is this a type (former) rather than a function?
    pub fn is_type(self) -> bool {
        matches!(
            self,
            Prim::Int
                | Prim::UInt
                | Prim::Float
                | Prim::Char
                | Prim::String
                | Prim::IO
                | Prim::IO1
                | Prim::Unique
                | Prim::Array
                | Prim::UArray
                | Prim::Region
                | Prim::Ref
                | Prim::Borrow
                | Prim::BorrowIO
                | Prim::Arena
                | Prim::RIO
                | Prim::In
                | Prim::RefIn
                | Prim::Lazy
                | Prim::ThreadBound
                | Prim::Thread
                | Prim::MVar
                | Prim::Chan
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    /// Indexed like the resolver's globals.
    pub globals: Vec<Global>,
    /// Names for printing (qualified).
    pub names: Vec<String>,
    pub main: Option<GName>,
    /// Definitions in elaboration order (for lowering).
    pub order: Vec<GName>,
    pub builtins: Builtins,
    /// Traits, by the name of their dictionary type.
    pub traits: HashMap<GName, TraitInfo>,
    /// Implementations of each trait.
    pub impls: HashMap<GName, Vec<ImplInfo>>,
    /// Declared totality of definitions (spec-v0.0.4.md §1); absent is
    /// `covering`.
    pub totality: HashMap<GName, (Totality, Span)>,
    /// `decreasing` measures of definitions.
    pub measures: HashMap<GName, Measure>,
    /// Where values are checked against refinement types: the `site` of
    /// each `primRefine` (spec-v0.0.4.md §3).
    pub refine_sites: Vec<RefineSite>,
    /// Every refinement was proved or is checked at run time
    /// (`--contracts=verify|check`): the operations that rely on proofs
    /// may drop their checks (spec-v0.0.4.md §5). `false` (the default)
    /// keeps the checks.
    pub proofs_trusted: bool,
}

/// A place where a value must satisfy a refinement.
#[derive(Clone, Debug)]
pub struct RefineSite {
    pub span: Span,
    /// The definition it is in.
    pub def: String,
}

/// A termination measure (spec-v0.0.4.md §1.3): a term of type `Int` or
/// `Nat` in the context of the first `nparams` parameters of the
/// definition (its leading lambdas).
#[derive(Clone, Debug)]
pub struct Measure {
    pub term: Term,
    pub nparams: usize,
    /// `Nat` (non-negative by construction) rather than `Int`.
    pub nat: bool,
    pub span: Span,
}

/// A trait: its dictionary type is the `Global::Data` of the same name,
/// with one constructor whose fields are the supertraits, then the methods
/// (spec-v0.0.2.md §3).
#[derive(Clone, Debug)]
pub struct TraitInfo {
    pub con: GName,
    /// Selector of each supertrait.
    pub supers: Vec<GName>,
    pub super_traits: Vec<GName>,
    /// Default definition of each method.
    pub defaults: Vec<Option<GName>>,
}

/// An instance: `dict : {vars} -> {ω ctx} -> Trait args`.
#[derive(Clone, Copy, Debug)]
pub struct ImplInfo {
    pub dict: GName,
    pub nvars: usize,
    pub nctx: usize,
}

/// Prelude types and constructors used by built-in sugar (literals,
/// `if`, tuples, lists, `do`) and primitive types.
#[derive(Clone, Debug, Default)]
pub struct Builtins {
    pub bool_: Option<GName>,
    pub true_: Option<GName>,
    pub false_: Option<GName>,
    pub unit: Option<GName>,
    pub mk_unit: Option<GName>,
    pub pair: Option<GName>,
    pub mk_pair: Option<GName>,
    pub list: Option<GName>,
    pub nil: Option<GName>,
    pub cons: Option<GName>,
    pub nat: Option<GName>,
    pub zero: Option<GName>,
    pub succ: Option<GName>,
    pub bind: Option<GName>,
    pub prim_bind: Option<GName>,
    pub then: Option<GName>,
    pub num: Option<GName>,
    pub from_int: Option<GName>,
    pub negate: Option<GName>,
    pub int: Option<GName>,
    pub uint: Option<GName>,
    pub float: Option<GName>,
    pub char_: Option<GName>,
    pub string: Option<GName>,
    pub io: Option<GName>,
    pub unique: Option<GName>,
    // IO with a linear result (spec-v0.0.2.md §5.2).
    pub io1: Option<GName>,
    pub prim_pure: Option<GName>,
    pub prim_pure1: Option<GName>,
    pub prim_bind1: Option<GName>,
    pub prim_bind_to1: Option<GName>,
    pub prim_bind11: Option<GName>,
    // Resources (§5.3).
    pub drop_trait: Option<GName>,
    pub release_trait: Option<GName>,
    pub drop_method: Option<GName>,
    pub release_method: Option<GName>,
    pub drop_free: Option<GName>,
    pub release_free: Option<GName>,
    pub release_by_drop: Option<GName>,
    pub drop_after: Option<GName>,
    pub release_after: Option<GName>,
    pub drop_after1: Option<GName>,
    pub release_after1: Option<GName>,
    // Errors (§6).
    pub result: Option<GName>,
    pub ok: Option<GName>,
    pub err: Option<GName>,
    // Unrestricted values (§5.6).
    pub mk_ur: Option<GName>,
    // Short-circuit operators (spec-v0.0.3.md §4.3).
    pub and: Option<GName>,
    pub or: Option<GName>,
    // Proofs (spec-v0.0.4.md §2).
    pub equal: Option<GName>,
    pub refl: Option<GName>,
    pub void: Option<GName>,
    pub rewrite: Option<GName>,
    /// `subsetOf a p`: the refinement type `{x : a | p x}` (spec-v0.0.4.md §3).
    pub subset_of: Option<GName>,
    /// `primRefine {a} p site x`: `x` checked against `{y : a | p y}`.
    pub refine: Option<GName>,
    // Threads (spec-v0.0.5.md §1).
    pub send_trait: Option<GName>,
    pub maybe: Option<GName>,
    pub nothing: Option<GName>,
    pub just: Option<GName>,
}

impl Program {
    pub fn get(&self, g: GName) -> &Global {
        &self.globals[g.0 as usize]
    }

    pub fn name(&self, g: GName) -> &str {
        &self.names[g.0 as usize]
    }

    /// Short (unqualified) name.
    pub fn short_name(&self, g: GName) -> &str {
        let n = self.name(g);
        match n.rfind('.') {
            Some(i) if i + 1 < n.len() && !n[..i].ends_with('.') => &n[i + 1..],
            _ => n,
        }
    }

    pub fn type_of(&self, g: GName) -> Option<&Term> {
        match self.get(g) {
            Global::Def { ty, .. }
            | Global::Data { ty, .. }
            | Global::Con { ty, .. }
            | Global::Prim { ty, .. }
            | Global::Extern { ty, .. } => Some(ty),
            Global::Missing => None,
        }
    }

    pub fn prim_of(&self, g: GName) -> Option<Prim> {
        match self.get(g) {
            Global::Prim { prim, .. } => Some(*prim),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------
// Printing

/// Prints a term with `names` giving the names of the variables in scope
/// (outermost first).
pub fn print_term(p: &Program, names: &mut Vec<Name>, t: &Term) -> String {
    let mut s = String::new();
    pt(p, names, t, 0, &mut s);
    s
}

fn fresh_name(names: &[Name], x: &Name) -> Name {
    if x.as_ref() == "_" {
        return x.clone();
    }
    let mut cand: Name = x.clone();
    let mut i = 1;
    while names.contains(&cand) {
        cand = format!("{x}{i}").into();
        i += 1;
    }
    cand
}

fn mult_prefix(m: Mult) -> &'static str {
    match m {
        Mult::Zero => "0 ",
        Mult::One => "1 ",
        Mult::Many => "",
    }
}

// Precedence: 0 = top (lambda, pi, let), 1 = application spine, 2 = atom.
fn pt(p: &Program, names: &mut Vec<Name>, t: &Term, prec: u8, s: &mut String) {
    let paren = |s: &mut String, open: bool| {
        if open {
            s.push('(');
        }
    };
    match t {
        Term::Var(i) => {
            let n = names.len() as i64 - 1 - *i as i64;
            match names.get(n as usize) {
                Some(x) if n >= 0 => s.push_str(x),
                _ => {
                    let _ = write!(s, "#{i}");
                }
            }
        }
        Term::Global(g) => s.push_str(p.short_name(*g)),
        Term::Type => s.push_str("Type"),
        Term::LType => s.push_str("LType"),
        Term::Lit(l) => {
            let _ = write!(s, "{l}");
        }
        Term::Meta(m) | Term::InsertedMeta(m, _) => {
            // Not numbered: the numbers change with the size of the Prelude.
            let _ = m;
            s.push('_');
        }
        Term::Error => s.push_str("<error>"),
        Term::App(..) => {
            let mut args = Vec::new();
            let mut h = t;
            while let Term::App(f, a, i) = h {
                args.push((a, *i));
                h = f;
            }
            args.reverse();
            if args.iter().all(|(_, i)| *i == Icit::Impl) {
                pt(p, names, h, prec, s);
                return;
            }
            paren(s, prec > 1);
            // Hide implicit arguments in the printed form.
            pt(p, names, h, 2, s);
            for (a, i) in args {
                if i == Icit::Impl {
                    continue;
                }
                s.push(' ');
                pt(p, names, a, 2, s);
            }
            if prec > 1 {
                s.push(')');
            }
        }
        Term::Pi(x, m, i, a, b) => {
            paren(s, prec > 0);
            let dependent = term_mentions(b, 0);
            let x2 = fresh_name(names, x);
            match (i, dependent || *m != Mult::Many) {
                (Icit::Impl, _) => {
                    let _ = write!(s, "{{{}{x2} : ", mult_prefix_impl(*m));
                    pt(p, names, a, 0, s);
                    s.push_str("} -> ");
                }
                (Icit::Expl, true) => {
                    let _ = write!(s, "({}{x2} : ", mult_prefix(*m));
                    pt(p, names, a, 0, s);
                    s.push_str(") -> ");
                }
                (Icit::Expl, false) => {
                    pt(p, names, a, 1, s);
                    s.push_str(" -> ");
                }
            }
            names.push(x2);
            pt(p, names, b, 0, s);
            names.pop();
            if prec > 0 {
                s.push(')');
            }
        }
        Term::Lam(x, _, i, _, b) => {
            paren(s, prec > 0);
            let x2 = fresh_name(names, x);
            if *i == Icit::Impl {
                let _ = write!(s, "\\{{{x2}}} => ");
            } else {
                let _ = write!(s, "\\{x2} => ");
            }
            names.push(x2);
            pt(p, names, b, 0, s);
            names.pop();
            if prec > 0 {
                s.push(')');
            }
        }
        Term::Let(x, _, _, v, b) => {
            paren(s, prec > 0);
            let x2 = fresh_name(names, x);
            let _ = write!(s, "let {x2} = ");
            pt(p, names, v, 0, s);
            s.push_str(" in ");
            names.push(x2);
            pt(p, names, b, 0, s);
            names.pop();
            if prec > 0 {
                s.push(')');
            }
        }
        Term::Case(c) => {
            paren(s, prec > 0);
            s.push_str("case ");
            pt(p, names, &c.scrut, 0, s);
            s.push_str(" of {");
            for (k, a) in c.alts.iter().enumerate() {
                if k > 0 {
                    s.push(';');
                }
                s.push(' ');
                match a {
                    Alt::Con {
                        con, fields, body, ..
                    } => {
                        s.push_str(p.short_name(*con));
                        let base = names.len();
                        for (x, m, _) in fields {
                            let x2 = fresh_name(names, x);
                            if *m == Mult::Zero {
                                let _ = write!(s, " {{{x2}}}");
                            } else {
                                let _ = write!(s, " {x2}");
                            }
                            names.push(x2);
                        }
                        s.push_str(" => ");
                        pt(p, names, body, 0, s);
                        names.truncate(base);
                    }
                    Alt::Lit { lit, body } => {
                        let _ = write!(s, "{lit} => ");
                        pt(p, names, body, 0, s);
                    }
                }
            }
            if let Some(d) = &c.default {
                s.push_str("; _ => ");
                pt(p, names, d, 0, s);
            }
            s.push_str(" }");
            if prec > 0 {
                s.push(')');
            }
        }
    }
}

fn mult_prefix_impl(m: Mult) -> &'static str {
    match m {
        Mult::Zero => "",
        Mult::One => "1 ",
        Mult::Many => "ω ",
    }
}

/// Does `t` mention the variable with index `i` (relative to `t`)?
pub fn term_mentions(t: &Term, i: u32) -> bool {
    match t {
        Term::Var(j) => *j == i,
        Term::Global(_) | Term::Type | Term::LType | Term::Lit(_) | Term::Meta(_) | Term::Error => {
            false
        }
        Term::InsertedMeta(..) => true,
        Term::Pi(_, _, _, a, b) | Term::Lam(_, _, _, a, b) => {
            term_mentions(a, i) || term_mentions(b, i + 1)
        }
        Term::App(f, a, _) => term_mentions(f, i) || term_mentions(a, i),
        Term::Let(_, _, a, v, b) => {
            term_mentions(a, i) || term_mentions(v, i) || term_mentions(b, i + 1)
        }
        Term::Case(c) => {
            term_mentions(&c.scrut, i)
                || term_mentions(&c.ret, i)
                || c.default.as_ref().is_some_and(|d| term_mentions(d, i))
                || c.alts.iter().any(|a| match a {
                    Alt::Con { fields, body, .. } => {
                        fields
                            .iter()
                            .enumerate()
                            .any(|(k, (_, _, ty))| term_mentions(ty, i + k as u32))
                            || term_mentions(body, i + fields.len() as u32)
                    }
                    Alt::Lit { body, .. } => term_mentions(body, i),
                })
        }
    }
}

/// Shifts the variables of `t` with index `>= cutoff` by `by` (for moving
/// a term under `by` new binders).
pub fn shift(t: &Term, cutoff: u32, by: u32) -> Term {
    let s = |t: &Term, c: u32| Rc::new(shift(t, c, by));
    match t {
        Term::Var(i) if *i >= cutoff => Term::Var(i + by),
        Term::Var(_)
        | Term::Global(_)
        | Term::Type
        | Term::LType
        | Term::Lit(_)
        | Term::Meta(_)
        | Term::InsertedMeta(..)
        | Term::Error => t.clone(),
        Term::Pi(x, m, i, a, b) => Term::Pi(x.clone(), *m, *i, s(a, cutoff), s(b, cutoff + 1)),
        Term::Lam(x, m, i, a, b) => Term::Lam(x.clone(), *m, *i, s(a, cutoff), s(b, cutoff + 1)),
        Term::App(f, a, i) => Term::App(s(f, cutoff), s(a, cutoff), *i),
        Term::Let(x, m, a, v, b) => {
            Term::Let(x.clone(), *m, s(a, cutoff), s(v, cutoff), s(b, cutoff + 1))
        }
        Term::Case(k) => Term::Case(Rc::new(Case {
            scrut: shift(&k.scrut, cutoff, by),
            ret: shift(&k.ret, cutoff, by),
            default: k.default.as_ref().map(|d| shift(d, cutoff, by)),
            alts: k
                .alts
                .iter()
                .map(|a| match a {
                    Alt::Con {
                        con,
                        fields,
                        refine,
                        body,
                    } => {
                        let n = fields.len() as u32;
                        Alt::Con {
                            con: *con,
                            fields: fields
                                .iter()
                                .enumerate()
                                .map(|(j, (x, m, ty))| {
                                    (x.clone(), *m, shift(ty, cutoff + j as u32, by))
                                })
                                .collect(),
                            refine: refine
                                .iter()
                                .map(|(ix, t)| {
                                    let ix = if *ix >= cutoff + n { ix + by } else { *ix };
                                    (ix, shift(t, cutoff + n, by))
                                })
                                .collect(),
                            body: shift(body, cutoff + n, by),
                        }
                    }
                    Alt::Lit { lit, body } => Alt::Lit {
                        lit: lit.clone(),
                        body: shift(body, cutoff, by),
                    },
                })
                .collect(),
        })),
    }
}

/// Prints a definition body for dumps (`--dump=tt`).
pub fn print_program(p: &Program) -> String {
    let mut s = String::new();
    for g in &p.order {
        let name = p.name(*g).to_string();
        match p.get(*g) {
            Global::Def { ty, body } => {
                let _ = writeln!(s, "{name} : {}", print_term(p, &mut Vec::new(), ty));
                if let Some(b) = body {
                    let _ = writeln!(s, "{name} = {}", print_term(p, &mut Vec::new(), b));
                }
            }
            Global::Data { ty, univ, .. } => {
                let _ = writeln!(
                    s,
                    "data {name} : {} ({:?})",
                    print_term(p, &mut Vec::new(), ty),
                    univ
                );
            }
            Global::Con { ty, .. } => {
                let _ = writeln!(s, "  con {name} : {}", print_term(p, &mut Vec::new(), ty));
            }
            Global::Prim { ty, .. } => {
                let _ = writeln!(s, "prim {name} : {}", print_term(p, &mut Vec::new(), ty));
            }
            Global::Extern { ty, abi } => {
                let _ = writeln!(
                    s,
                    "extern {abi:?} {name} : {}",
                    print_term(p, &mut Vec::new(), ty)
                );
            }
            Global::Missing => {}
        }
    }
    s
}
