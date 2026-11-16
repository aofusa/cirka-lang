//! Surface syntax of CirkaLang v0.0.1 (spec-v0.0.1.md §3).
//!
//! Operator applications are kept as flat sequences ([`ExprKind::OpSeq`],
//! [`PatKind::OpSeq`]) because fixities may come from other modules; the
//! resolver re-associates them.

use cirka_diagnostics::Span;

/// A possibly qualified name: `x`, `A.B.x`, `+`, `A.+`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QName {
    pub module: Vec<String>,
    pub name: String,
}

impl QName {
    pub fn simple(name: impl Into<String>) -> QName {
        QName {
            module: Vec::new(),
            name: name.into(),
        }
    }

    /// Names starting with an upper-case letter or `:` are constructors or
    /// types.
    pub fn is_con(&self) -> bool {
        self.name
            .starts_with(|c: char| c.is_ascii_uppercase() || c == ':')
    }

    pub fn is_op(&self) -> bool {
        !self
            .name
            .starts_with(|c: char| c.is_alphanumeric() || c == '_')
    }
}

impl std::fmt::Display for QName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for m in &self.module {
            write!(f, "{m}.")?;
        }
        write!(f, "{}", self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mult {
    Zero,
    One,
    Many,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    /// `module A.B` (absent when omitted).
    pub name: Option<(Vec<String>, Span)>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub path: Vec<String>,
    /// `import A.B as X`: the names of `A.B` are visible only as `X.name`
    /// (spec-v0.0.2.md §7.1).
    pub alias: Option<(Vec<String>, Span)>,
    /// `import A.B (x, (+))`: only these names are visible unqualified
    /// (with an alias, in addition to `X.name`).
    pub names: Option<Vec<(String, Span)>>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub private: bool,
    pub origin: Origin,
    pub kind: DeclKind,
    pub span: Span,
}

/// Where a declaration comes from (FEAT-0255). The source decides what a
/// program uses: a declaration expanded from an extern block is part of
/// the program only when the source uses it (spec-v0.0.6.md §1.10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// Written in the source, or derived for a type written there.
    #[default]
    Source,
    /// Expanded from a declaration of an extern block and checked like the
    /// source, since its errors are the declaration's: type aliases,
    /// constants and enums.
    Extern,
    /// Expanded from a struct or union of an extern block, which is
    /// well-typed when the declaration is (§1.9): only checked when used.
    ExternLazy,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclKind {
    Fixity {
        assoc: Assoc,
        prec: u8,
        ops: Vec<(String, Span)>,
    },
    Sig {
        names: Vec<(String, Span)>,
        ty: Expr,
        /// `total` / `covering` / `partial` before the signature
        /// (spec-v0.0.4.md §1).
        totality: Option<(Totality, Span)>,
        /// `trusted` before the signature (spec-v0.0.5.md §4.2): the
        /// refinements of the result and `total` are assumed, not proved.
        trusted: Option<Span>,
    },
    Clause {
        name: String,
        name_span: Span,
        /// Explicit argument patterns.
        pats: Vec<Pat>,
        rhs: Expr,
        /// `decreasing e` after the right-hand side: a termination measure
        /// (spec-v0.0.4.md §1.3).
        measure: Option<Expr>,
    },
    Data(DataDecl),
    Prim {
        name: String,
        name_span: Span,
        ty: Expr,
    },
    /// `test "name" = body` (12-cirkalang.md §11): `body : IO ()`.
    Test {
        name: String,
        name_span: Span,
        body: Expr,
    },
    /// `tests where { decls }`: declarations that exist only in test builds.
    Tests(Vec<Decl>),
    /// `extern "C" [link "lib"] where { name : type; .. }` (spec-v0.0.2.md
    /// §8): functions of another language, called by their names.
    Extern(ExternBlock),
    /// `trait [ctx =>] Name params where { sigs; default clauses }`.
    Trait(TraitDecl),
    /// `impl [ctx =>] Trait args [where { clauses }]`.
    Impl(ImplDecl),
}

/// What a function promises about its evaluation (spec-v0.0.4.md §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Totality {
    /// Terminates on every argument without failing.
    Total,
    /// Its patterns are exhaustive (the default).
    Covering,
    /// May have missing cases (they panic).
    Partial,
}

/// The calling convention of an `extern` block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abi {
    /// `"C"`: the C ABI of the platform (scalar C types).
    C,
    /// `"cirka"`: the runtime's convention (`String` is passed as the
    /// runtime's string object). Used by the standard library.
    Cirka,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternBlock {
    pub abi: Abi,
    pub abi_span: Span,
    /// `link "m"`: the libraries that define the functions.
    pub links: Vec<(String, Span)>,
    /// `name : type`.
    pub sigs: Vec<ExternSig>,
    /// The other declarations (spec-v0.0.6.md §1.1), expanded by
    /// [`crate::cextern`].
    pub items: Vec<ExternItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternSig {
    pub name: String,
    pub name_span: Span,
    /// `"sym" name : type`: the C symbol when it is not the name, or
    /// `"dynamic"`, `"wrapper"`, `"&sym"` (spec-v0.0.6.md §1.5).
    pub symbol: Option<(String, Span)>,
    /// The type without the `...` of a variadic function.
    pub ty: Expr,
    /// `...`: the number of fixed parameters (spec-v0.0.6.md §1.4).
    pub variadic: Option<(usize, Span)>,
}

/// A declaration of an `extern` block other than a function.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternItem {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub kind: ExternItemKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExternItemKind {
    /// `type T`: a type without values (pointed to only).
    Opaque,
    /// `type T = ty` (a `typedef`).
    Alias(Expr),
    /// `const X : ty = literal`.
    Const { ty: Expr, value: Expr },
    /// `enum E : ty where { A = n; .. }`.
    Enum {
        repr: Expr,
        consts: Vec<ExternConst>,
    },
    /// `struct S where { f : ty; .. }`, `packed struct`, `union`.
    Record {
        union: bool,
        packed: bool,
        fields: Vec<ExternField>,
    },
}

/// `A = n` of an `enum`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternConst {
    pub name: String,
    pub span: Span,
    /// An integer literal, possibly negated.
    pub value: Expr,
}

/// `f : ty` of a `struct` or `union`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternField {
    pub name: String,
    pub span: Span,
    pub ty: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitDecl {
    /// Supertraits (`Eq a` in `trait Eq a => Ord a`).
    pub context: Vec<Expr>,
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    /// `Sig` and `Clause` declarations.
    pub items: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplDecl {
    /// Declared variables `{a : LType} ->` (spec-v0.0.3.md §2.4).
    pub binders: Vec<(String, Span, Expr)>,
    pub context: Vec<Expr>,
    /// `Trait arg1 ... argn`.
    pub head: Expr,
    /// `Clause` declarations.
    pub items: Vec<Decl>,
}

/// `(π x y : A)` in a data declaration's parameter list.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub mult: Option<Mult>,
    pub names: Vec<(String, Span)>,
    /// `None` for the short form's bare parameters (`data Maybe a = ...`).
    pub ty: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    /// The signature after `:` in the GADT form (`Type` when omitted).
    pub sig: Option<Expr>,
    pub cons: Vec<ConDecl>,
    /// Declared with the short form `data T a = A | B x`.
    pub short: bool,
    /// `deriving (Eq, Show)`.
    pub deriving: Vec<(QName, Span)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConDecl {
    pub name: String,
    pub span: Span,
    pub body: ConBody,
    /// `private`: only usable in its module (spec-v0.0.6.md §2).
    pub private: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConBody {
    /// GADT form: the constructor's full type.
    Sig(Expr),
    /// Short form: the field types.
    Fields(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// Integer literal (absolute value; `-` is applied by the parser for
    /// negative patterns).
    Int(u128),
    Float(f64),
    Char(char),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// A binder of a lambda: `x`, `_` or `(π x : A)`.
#[derive(Clone, Debug, PartialEq)]
pub struct LamBinder {
    pub name: Option<String>,
    pub span: Span,
    pub mult: Option<Mult>,
    pub ty: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LetBind {
    pub mult: Option<Mult>,
    pub name: String,
    pub name_span: Span,
    pub ty: Option<Expr>,
    /// `let f x y = e`: argument patterns (variables or `_`).
    pub args: Vec<Pat>,
    pub rhs: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alt {
    pub pat: Pat,
    pub rhs: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Bind(String, Span, Expr),
    Let(Vec<LetBind>),
    Expr(Expr),
    /// `x <- e?` / `e?`: run `e` and return its error early
    /// (spec-v0.0.2.md §6.2). The span is that of the `?`.
    Try(Option<(String, Span)>, Expr, Span),
    /// `p <- e | q => h | ...` with a pattern other than a variable (`_`,
    /// `(a, b)`, `Ok x`): the rest of the block runs in the alternative
    /// for `p` of a `case` of the result, the other alternatives end the
    /// block (spec-v0.0.3.md §5).
    BindPat(Pat, Expr, Vec<Alt>),
}

/// An operator occurrence in an operator sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct OpRef {
    pub name: QName,
    pub span: Span,
    /// Written with backticks (`` `div` ``).
    pub backtick: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Var(QName),
    Lit(Lit),
    Hole(String),
    Wildcard,
    Type,
    LType,
    Unit,
    App(Box<Expr>, Box<Expr>),
    Lam(Vec<LamBinder>, Box<Expr>),
    Let(Vec<LetBind>, Box<Expr>),
    Case(Box<Expr>, Vec<Alt>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Do(Vec<Stmt>),
    /// `(π x y : A) -> B`, `{x : A} -> B`, or `A -> B` (no names).
    Pi {
        mult: Option<Mult>,
        implicit: bool,
        names: Vec<(String, Span)>,
        dom: Box<Expr>,
        cod: Box<Expr>,
    },
    Ann(Box<Expr>, Box<Expr>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    /// `e1 op1 e2 op2 e3 ...` with a leading `-` when `neg` is set.
    OpSeq {
        neg: Option<Span>,
        operands: Vec<Expr>,
        ops: Vec<OpRef>,
    },
    /// `(e op)`.
    LeftSection(Box<Expr>, OpRef),
    /// `(op e)`.
    RightSection(OpRef, Box<Expr>),
    /// `x = y`: propositional equality (spec-v0.0.4.md §2.1).
    Equal(Box<Expr>, Box<Expr>),
    /// `{x : A | p}`: a refinement (subset) type (spec-v0.0.4.md §3).
    Subset(String, Span, Box<Expr>, Box<Expr>),
    /// `rewrite p in e` (spec-v0.0.4.md §2.2).
    Rewrite(Box<Expr>, Box<Expr>),
    /// `with e of alts`: a match that abstracts `e` in the goal
    /// (spec-v0.0.4.md §2.3).
    With(Box<Expr>, Vec<Alt>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatKind {
    /// A variable, or a constructor without arguments when the name is
    /// upper-case (decided by the resolver for lower-case names never).
    Var(String),
    Wild,
    Con(QName, Vec<Pat>),
    /// Literal, with sign for integers (`-1`).
    Lit(Lit, bool),
    Unit,
    Tuple(Vec<Pat>),
    List(Vec<Pat>),
    /// `p1 :: p2 :: p3` (constructor operators).
    OpSeq(Vec<Pat>, Vec<OpRef>),
}
