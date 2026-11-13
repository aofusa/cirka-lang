//! The resolved program: every name refers to a local variable or a
//! global, operator sequences are associated by fixity, sections and
//! `let f x = e` are desugared.

use cirka_diagnostics::{FileId, Span};
pub use cirkalang_syntax::ast::Abi;
pub use cirkalang_syntax::ast::{Lit, Mult};

/// A global: index into [`Program::globals`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GName(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GKind {
    /// A function or value defined by clauses.
    Def,
    /// A data type.
    Data,
    /// A data constructor.
    Con,
    /// A primitive (`prim name : T`).
    Prim,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GlobalInfo {
    /// Dotted module name.
    pub module: String,
    pub name: String,
    pub kind: GKind,
    pub span: Span,
    pub private: bool,
}

impl GlobalInfo {
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Program {
    pub globals: Vec<GlobalInfo>,
    /// Modules in dependency order (imports first).
    pub modules: Vec<Module>,
    /// `main` of the root module (in test builds, the generated test main).
    pub main: Option<GName>,
    /// The tests of all modules, in dependency order (empty unless tests
    /// are kept: 12-cirkalang.md §11).
    pub tests: Vec<Test>,
}

/// A `test "name" = e` declaration, defined as the private global `def`.
#[derive(Clone, Debug, PartialEq)]
pub struct Test {
    pub module: String,
    pub name: String,
    pub def: GName,
    pub span: Span,
}

impl Program {
    pub fn info(&self, g: GName) -> &GlobalInfo {
        &self.globals[g.0 as usize]
    }

    /// Looks up a global by module and name (used for built-in sugar).
    pub fn find(&self, module: &str, name: &str) -> Option<GName> {
        self.globals
            .iter()
            .position(|g| g.module == module && g.name == name)
            .map(|i| GName(i as u32))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub file: FileId,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Def(Def),
    Data(Data),
    Prim(GName, Expr),
    /// A function of an `extern` block (spec-v0.0.2.md §8).
    Extern(GName, Expr, Abi),
    /// A trait; its dictionary type is an `Item::Data` of the same name.
    Trait(Trait),
    Impl(Impl),
}

/// `trait ... where ...` (spec-v0.0.2.md §3): the dictionary type and its
/// constructor are ordinary data; this records what the elaborator must
/// generate besides them. Dictionary fields are the supertraits, then the
/// methods.
#[derive(Clone, Debug, PartialEq)]
pub struct Trait {
    pub name: GName,
    pub dict_con: GName,
    /// Source names of the parameters (in scope in default clauses).
    pub params: Vec<String>,
    /// Selector of each supertrait.
    pub supers: Vec<GName>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitMethod {
    /// The selector, named like the method.
    pub name: GName,
    /// Default definition: its global and clauses.
    pub default: Option<(GName, Vec<Clause>)>,
    pub span: Span,
}

/// `impl [ctx =>] Trait args where ...`.
#[derive(Clone, Debug, PartialEq)]
pub struct Impl {
    /// The dictionary.
    pub name: GName,
    /// Variables of the head, implicitly bound.
    pub vars: Vec<(String, Span)>,
    pub context: Vec<Expr>,
    /// The trait applied to its arguments.
    pub head: Expr,
    pub trait_: GName,
    /// One entry per method of the trait, in field order.
    pub methods: Vec<ImplMethod>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplMethod {
    /// The trait's selector.
    pub method: GName,
    /// The global holding this implementation.
    pub def: GName,
    /// Empty when the default is used.
    pub clauses: Vec<Clause>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    pub name: GName,
    /// The signature, with auto-bound implicit arguments made explicit.
    pub sig: Expr,
    pub clauses: Vec<Clause>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub pats: Vec<Pat>,
    pub rhs: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub name: GName,
    pub params: Vec<Param>,
    pub sig: Option<Expr>,
    pub cons: Vec<Con>,
    pub short: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub mult: Option<Mult>,
    pub name: String,
    pub span: Span,
    /// `None` for short-form parameters (of type `Type`).
    pub ty: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Con {
    pub name: GName,
    pub span: Span,
    pub body: ConBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConBody {
    Sig(Expr),
    Fields(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

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
    pub span: Span,
    pub ty: Option<Expr>,
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
    /// `x <- e?` / `e?` (the span is that of the `?`).
    Try(Option<(String, Span)>, Expr, Span),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Local(String),
    Global(GName),
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
    /// One binder per `Pi` (`(x, y : A) -> B` is split).
    Pi {
        mult: Option<Mult>,
        implicit: bool,
        name: Option<String>,
        name_span: Span,
        dom: Box<Expr>,
        cod: Box<Expr>,
    },
    Ann(Box<Expr>, Box<Expr>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatKind {
    Var(String),
    Wild,
    Con(GName, Vec<Pat>),
    /// Literal; `true` when negated (`-1`).
    Lit(Lit, bool),
    Unit,
    Tuple(Vec<Pat>),
    List(Vec<Pat>),
}
