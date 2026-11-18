//! The resolved program: every name refers to a local variable or a
//! global, operator sequences are associated by fixity, sections and
//! `let f x = e` are desugared.

use cirka_diagnostics::{FileId, Span};
pub use cirkalang_syntax::ast::Abi;
pub use cirkalang_syntax::ast::Origin;
pub use cirkalang_syntax::ast::{Lit, Mult, Totality};

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
    /// The capabilities it receives (spec-v0.0.7.md §1.2).
    pub caps: Vec<String>,
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
    Extern(ExternFn),
    /// A struct or union of an `extern` block (spec-v0.0.6.md §1.3); its
    /// data type and functions are ordinary items.
    CRecord(CRecord),
    /// A trait; its dictionary type is an `Item::Data` of the same name.
    Trait(Trait),
    Impl(Impl),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternFn {
    pub name: GName,
    pub ty: Expr,
    pub abi: Abi,
    /// `"sym" name : type`: the C symbol when it is not the name, or
    /// `"dynamic"`, `"wrapper"`, `"&sym"` (spec-v0.0.6.md §1.5).
    pub symbol: Option<(String, Span)>,
    /// The number of fixed parameters of a variadic function.
    pub variadic: Option<(usize, Span)>,
    /// The libraries of the block (`link "lib"`).
    pub links: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CRecord {
    pub data: GName,
    pub union: bool,
    pub packed: bool,
    /// `T.$peek : Int -> IO T` and `T.$poke : Int -> T -> IO ()`.
    pub peek: GName,
    pub poke: GName,
    /// Of a union: the member pointers `U.m : Ptr U -> Ptr M`, whose types
    /// give the members.
    pub members: Vec<GName>,
    /// The names and places of the fields (members).
    pub fields: Vec<(String, Span)>,
    pub span: Span,
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
    /// The declared sorts of the first variables (`impl {a : LType} ->`).
    pub kinds: Vec<Expr>,
    pub context: Vec<Expr>,
    /// The trait applied to its arguments.
    pub head: Expr,
    pub trait_: GName,
    /// One entry per method of the trait, in field order.
    pub methods: Vec<ImplMethod>,
    pub span: Span,
    pub origin: Origin,
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
    /// `total` / `covering` / `partial` (spec-v0.0.4.md §1); `None` is
    /// `covering`.
    pub totality: Option<(Totality, Span)>,
    /// `trusted` (spec-v0.0.5.md §4.2): the refinements of the result and
    /// `total` are assumed, not proved. Only in the libraries.
    pub trusted: Option<Span>,
    pub origin: Origin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub pats: Vec<Pat>,
    pub rhs: Expr,
    pub span: Span,
    /// `decreasing e`, in the scope of the clause's patterns.
    pub measure: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub name: GName,
    pub params: Vec<Param>,
    pub sig: Option<Expr>,
    pub cons: Vec<Con>,
    pub short: bool,
    pub span: Span,
    pub origin: Origin,
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
    /// `rewrite p in e` (spec-v0.0.4.md §2.2).
    Rewrite(Box<Expr>, Box<Expr>),
    /// `with e of alts` (spec-v0.0.4.md §2.3).
    With(Box<Expr>, Vec<Alt>),
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

/// Calls `f` with every global that `e` names, constructors in patterns
/// included.
pub fn globals_in_expr(e: &Expr, f: &mut dyn FnMut(GName)) {
    match &e.kind {
        ExprKind::Global(g) => f(*g),
        ExprKind::Local(_)
        | ExprKind::Lit(_)
        | ExprKind::Hole(_)
        | ExprKind::Wildcard
        | ExprKind::Type
        | ExprKind::LType
        | ExprKind::Unit => {}
        ExprKind::App(a, b) | ExprKind::Ann(a, b) | ExprKind::Rewrite(a, b) => {
            globals_in_expr(a, f);
            globals_in_expr(b, f);
        }
        ExprKind::Lam(bs, b) => {
            for t in bs.iter().filter_map(|b| b.ty.as_ref()) {
                globals_in_expr(t, f);
            }
            globals_in_expr(b, f);
        }
        ExprKind::Let(bs, b) => {
            globals_in_lets(bs, f);
            globals_in_expr(b, f);
        }
        ExprKind::Case(s, alts) | ExprKind::With(s, alts) => {
            globals_in_expr(s, f);
            for a in alts {
                globals_in_pat(&a.pat, f);
                globals_in_expr(&a.rhs, f);
            }
        }
        ExprKind::If(c, t, e) => {
            globals_in_expr(c, f);
            globals_in_expr(t, f);
            globals_in_expr(e, f);
        }
        ExprKind::Do(ss) => {
            for s in ss {
                match s {
                    Stmt::Bind(_, _, x) | Stmt::Expr(x) | Stmt::Try(_, x, _) => {
                        globals_in_expr(x, f)
                    }
                    Stmt::Let(bs) => globals_in_lets(bs, f),
                }
            }
        }
        ExprKind::Pi { dom, cod, .. } => {
            globals_in_expr(dom, f);
            globals_in_expr(cod, f);
        }
        ExprKind::Tuple(xs) | ExprKind::List(xs) => {
            for x in xs {
                globals_in_expr(x, f);
            }
        }
    }
}

fn globals_in_lets(bs: &[LetBind], f: &mut dyn FnMut(GName)) {
    for b in bs {
        if let Some(t) = &b.ty {
            globals_in_expr(t, f);
        }
        globals_in_expr(&b.rhs, f);
    }
}

/// Calls `f` with the constructors of `p`.
pub fn globals_in_pat(p: &Pat, f: &mut dyn FnMut(GName)) {
    match &p.kind {
        PatKind::Con(c, ps) => {
            f(*c);
            for p in ps {
                globals_in_pat(p, f);
            }
        }
        PatKind::Tuple(ps) | PatKind::List(ps) => {
            for p in ps {
                globals_in_pat(p, f);
            }
        }
        PatKind::Var(_) | PatKind::Wild | PatKind::Lit(..) | PatKind::Unit => {}
    }
}

/// Calls `f` with the globals of clauses.
pub fn globals_in_clauses(cs: &[Clause], f: &mut dyn FnMut(GName)) {
    for c in cs {
        for p in &c.pats {
            globals_in_pat(p, f);
        }
        globals_in_expr(&c.rhs, f);
        if let Some(m) = &c.measure {
            globals_in_expr(m, f);
        }
    }
}
