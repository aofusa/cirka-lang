//! Abstract syntax of Haskell 2010 modules. Operator applications are kept
//! as flat sequences; fixity resolution happens in the renamer.

use cirka_diagnostics::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A possibly qualified name (`x`, `M.x`, `+`, `M.:+`).
#[derive(Clone, Debug, PartialEq)]
pub struct QName {
    pub qual: Option<String>,
    pub name: String,
    pub span: Span,
}

impl QName {
    pub fn unqual(name: impl Into<String>, span: Span) -> QName {
        QName {
            qual: None,
            name: name.into(),
            span,
        }
    }

    /// Constructor names start with an upper-case letter or `:`; the
    /// special constructors are `()`, `[]` and `(,..)`.
    pub fn is_con(&self) -> bool {
        is_con_name(&self.name)
    }

    pub fn text(&self) -> String {
        match &self.qual {
            Some(q) => format!("{q}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

pub fn is_con_name(n: &str) -> bool {
    n.starts_with(|c: char| c.is_uppercase() || c == ':' || c == '(' || c == '[')
}

pub fn is_op_name(n: &str) -> bool {
    !n.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '(' || c == '[')
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: Option<Ident>,
    pub exports: Option<Vec<Export>>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
    /// `{-# LANGUAGE ... #-}` extensions.
    pub extensions: Vec<(String, Span)>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Export {
    Var(QName),
    /// `T`, `T(..)`, `T(A, b)`.
    Thing(QName, Option<ItemList>),
    Module(Ident),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemList {
    All,
    Some(Vec<Ident>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub module: Ident,
    pub qualified: bool,
    pub alias: Option<Ident>,
    pub hiding: bool,
    pub items: Option<Vec<ImportItem>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportItem {
    Var(Ident),
    Thing(Ident, Option<ItemList>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decl {
    TypeSig(Vec<Ident>, Type, Span),
    Fixity(Assoc, u8, Vec<Ident>, Span),
    /// One clause of a function: `f p1 .. pn rhs` or `p1 `op` p2 rhs`.
    Fun(Ident, Vec<Pat>, Rhs, Span),
    /// A pattern binding `p = e` (a variable binding is `Fun` with no
    /// arguments).
    PatBind(Pat, Rhs, Span),
    Data(DataDecl),
    TypeSyn(Ident, Vec<Ident>, Type, Span),
    Class(ClassDecl),
    Instance(InstDecl),
    Default(Vec<Type>, Span),
    Foreign(ForeignDecl),
}

impl Decl {
    pub fn span(&self) -> Span {
        match self {
            Decl::TypeSig(_, _, s)
            | Decl::Fixity(_, _, _, s)
            | Decl::Fun(_, _, _, s)
            | Decl::PatBind(_, _, s)
            | Decl::TypeSyn(_, _, _, s)
            | Decl::Default(_, s) => *s,
            Decl::Data(d) => d.span,
            Decl::Class(c) => c.span,
            Decl::Instance(i) => i.span,
            Decl::Foreign(f) => f.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataDecl {
    pub newtype: bool,
    pub context: Vec<Constraint>,
    pub name: Ident,
    pub params: Vec<Ident>,
    /// Kind signatures of the parameters (`data P (f :: Type -> Type)`).
    pub param_kinds: Vec<Option<KindSig>>,
    pub cons: Vec<ConDecl>,
    pub deriving: Vec<QName>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConDecl {
    pub name: Ident,
    pub fields: ConFields,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConFields {
    /// Positional fields; `true` marks a strict field (`!T`).
    Plain(Vec<(Type, bool)>),
    Record(Vec<(Vec<Ident>, Type, bool)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDecl {
    pub context: Vec<Constraint>,
    pub name: Ident,
    pub params: Vec<Ident>,
    pub param_kinds: Vec<Option<KindSig>>,
    pub decls: Vec<Decl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstDecl {
    pub context: Vec<Constraint>,
    pub class: QName,
    pub types: Vec<Type>,
    pub decls: Vec<Decl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForeignDecl {
    pub import: bool,
    pub callconv: Ident,
    pub entity: Option<String>,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// A class constraint `C t1 .. tn`.
#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub class: QName,
    pub types: Vec<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Var(String),
    /// Type constructor, including `()`, `[]`, `->`, `(,)`.
    Con(QName),
    App(Box<Type>, Box<Type>),
    Fun(Box<Type>, Box<Type>),
    List(Box<Type>),
    Tuple(Vec<Type>),
    /// `ctx => t` (in signatures).
    Qual(Vec<Constraint>, Box<Type>),
    /// `forall a b. t` (RankNTypes / ScopedTypeVariables).
    Forall(Vec<Ident>, Box<Type>),
    /// `(t :: k)` (KindSignatures).
    Kinded(Box<Type>, KindSig),
}

/// A kind written in the source (KindSignatures): `*` / `Type` and arrows.
#[derive(Clone, Debug, PartialEq)]
pub enum KindSig {
    Star,
    Arrow(Box<KindSig>, Box<KindSig>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rhs {
    pub body: RhsBody,
    pub wheres: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RhsBody {
    Plain(Expr),
    Guarded(Vec<GuardedExpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuardedExpr {
    pub guards: Vec<Stmt>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(u128),
    Frac(f64),
    Char(char),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// An element of an unresolved operator sequence.
#[derive(Clone, Debug, PartialEq)]
pub enum OpItem<T> {
    Operand(T),
    /// A binary operator (symbol or back-quoted identifier).
    Op(QName),
    /// Prefix `-` (expressions and negative literal patterns).
    Neg(Span),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Var(QName),
    Con(QName),
    Lit(Lit),
    App(Box<Expr>, Box<Expr>),
    /// `e1 op e2 op ...` before fixity resolution.
    OpSeq(Vec<OpItem<Expr>>),
    Lam(Vec<Pat>, Box<Expr>),
    Let(Vec<Decl>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `if | g -> e | ...` (MultiWayIf).
    MultiIf(Vec<GuardedExpr>),
    Case(Box<Expr>, Vec<Alt>),
    /// `\case` (LambdaCase).
    LamCase(Vec<Alt>),
    Do(Vec<Stmt>),
    Tuple(Vec<Expr>),
    /// `(, e)` / `(e ,)` (TupleSections); `None` is a missing component.
    TupleSection(Vec<Option<Expr>>),
    List(Vec<Expr>),
    /// `[a ..]`, `[a, b ..]`, `[a .. c]`, `[a, b .. c]`.
    Range(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
    ListComp(Box<Expr>, Vec<Stmt>),
    /// `(e op)`.
    LeftSection(Box<Expr>, QName),
    /// `(op e)`.
    RightSection(QName, Box<Expr>),
    Typed(Box<Expr>, Type),
    RecordCon(QName, Vec<(QName, Option<Expr>)>),
    RecordUpd(Box<Expr>, Vec<(QName, Expr)>),
    /// Parentheses (kept so that `(a + b) * c` is not re-associated).
    Paren(Box<Expr>),
    /// Placeholder after a syntax error.
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alt {
    pub pat: Pat,
    pub rhs: Rhs,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Bind(Pat, Expr, Span),
    Let(Vec<Decl>, Span),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatKind {
    Var(Ident),
    Wild,
    Lit(Lit),
    /// `-1`, `-1.5`.
    NegLit(Lit),
    Con(QName, Vec<Pat>),
    /// `p1 : p2` etc. before fixity resolution.
    OpSeq(Vec<OpItem<Pat>>),
    Tuple(Vec<Pat>),
    List(Vec<Pat>),
    As(Ident, Box<Pat>),
    Lazy(Box<Pat>),
    Bang(Box<Pat>),
    Record(QName, Vec<(QName, Option<Pat>)>),
    Paren(Box<Pat>),
    Typed(Box<Pat>, Type),
    Error,
}
