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
    pub kind: DeclKind,
    pub span: Span,
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
    },
    Clause {
        name: String,
        name_span: Span,
        /// Explicit argument patterns.
        pats: Vec<Pat>,
        rhs: Expr,
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConDecl {
    pub name: String,
    pub span: Span,
    pub body: ConBody,
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
