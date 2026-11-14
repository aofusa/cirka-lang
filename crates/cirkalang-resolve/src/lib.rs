//! cirkalang-resolve: module loading, scopes and name resolution
//! (docs/design/cirkalang/spec-v0.0.1.md §1, §3.1).
//!
//! The result ([`rast::Program`]) has every name resolved to a local or a
//! global, operator sequences associated by fixity, sections and
//! `let f x = e` desugared, and auto-bound implicit arguments of
//! signatures made explicit.

pub mod rast;

use cirka_diagnostics::{Diagnostic, FileId, SourceMap, Span};
use cirkalang_syntax::ast;
use rast::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Finds module sources.
pub trait Loader {
    /// Loads module `path` (e.g. `["Core", "List"]`), adding it to `sm`.
    fn load(&mut self, sm: &mut SourceMap, path: &[String]) -> Option<(FileId, String)>;

    /// Why module `importer` may not import `path`, if it may not (a
    /// package that does not depend on the owner of `path`).
    fn forbidden(&self, _path: &[String], _importer: &str) -> Option<String> {
        None
    }

    /// Why module `path` was not found, if something is known about it.
    fn not_found_hint(&self, _path: &[String]) -> Option<String> {
        None
    }
}

/// Loads `A/B.cirka` from the first root that has it.
pub struct FsLoader {
    pub roots: Vec<PathBuf>,
}

impl Loader for FsLoader {
    fn load(&mut self, sm: &mut SourceMap, path: &[String]) -> Option<(FileId, String)> {
        for r in &self.roots {
            let mut p = r.clone();
            for (i, seg) in path.iter().enumerate() {
                if i + 1 == path.len() {
                    p.push(format!("{seg}.cirka"));
                } else {
                    p.push(seg);
                }
            }
            if let Ok(text) = std::fs::read_to_string(&p) {
                let id = sm.add(p.display().to_string(), text.clone());
                return Some((id, text));
            }
        }
        None
    }
}

/// In-memory sources (tests): module name → text.
pub struct MemLoader(pub HashMap<String, String>);

impl Loader for MemLoader {
    fn load(&mut self, sm: &mut SourceMap, path: &[String]) -> Option<(FileId, String)> {
        let name = path.join(".");
        let text = self.0.get(&name)?.clone();
        let id = sm.add(format!("{}.cirka", path.join("/")), text.clone());
        Some((id, text))
    }
}

/// What to do with the tests in source files (12-cirkalang.md §11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TestMode {
    /// Remove them right after parsing (`build`, `run`).
    #[default]
    Strip,
    /// Keep and check them (`check`).
    Check,
    /// Keep them and make `main` run the test chosen by the command line
    /// (`test`).
    Run,
}

pub struct Options {
    /// Import `Prelude` implicitly (when it can be found).
    pub prelude: bool,
    pub tests: TestMode,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            prelude: true,
            tests: TestMode::Strip,
        }
    }
}

/// Parses and resolves the program whose root module is in `file`
/// (already added to `sm`). Other modules come from `loader`.
pub fn resolve_program(
    sm: &mut SourceMap,
    root: FileId,
    root_text: &str,
    loader: &mut dyn Loader,
    opts: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let mut st = State {
        loaded: HashMap::new(),
        order: Vec::new(),
        errs: Vec::new(),
        visiting: Vec::new(),
        tests: opts.tests,
        test_decls: HashMap::new(),
    };
    let (m, errs) = cirkalang_syntax::parse(root, root_text);
    st.errs.extend(errs);
    let root_name = m
        .name
        .as_ref()
        .map(|(p, _)| p.join("."))
        .unwrap_or_else(|| "Main".to_string());
    let has_prelude = opts.prelude && root_name != "Prelude" && {
        let mut probe = SourceMap::new();
        loader.load(&mut probe, &["Prelude".to_string()]).is_some()
    };
    st.add_module(sm, loader, root_name.clone(), root, m, has_prelude, None);
    if st.errs.iter().any(|e| e.is_error()) {
        return Err(st.errs);
    }
    let mut r = Resolver::new(&st);
    r.declare_all();
    r.collect_fixities();
    let mut prog = Program {
        globals: r.globals.clone(),
        modules: Vec::new(),
        main: None,
        tests: Vec::new(),
    };
    for name in st.order.clone() {
        let m = r.module(&name);
        prog.modules.push(m);
    }
    prog.globals = r.globals.clone();
    prog.main = r.tables[&root_name]
        .own
        .get("main")
        .copied()
        .filter(|g| r.globals[g.0 as usize].kind == GKind::Def);
    if opts.tests != TestMode::Strip {
        r.check_test_only(&prog);
        prog.tests = r.tests(&st.order);
    }
    if opts.tests == TestMode::Run {
        r.test_main(&mut prog, &root_name);
    }
    let rerrs = std::mem::take(&mut r.errs);
    drop(r);
    let mut errs = st.errs;
    errs.extend(rerrs);
    if errs.iter().any(|e| e.is_error()) {
        return Err(errs);
    }
    Ok(prog)
}

/// Convenience: reads `main_file`, uses its directory and `lib_roots` as
/// source roots.
pub fn resolve_file(
    sm: &mut SourceMap,
    main_file: &Path,
    lib_roots: &[PathBuf],
    opts: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let mut roots = vec![
        main_file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default(),
    ];
    roots.extend(lib_roots.iter().cloned());
    resolve_main(sm, main_file, &mut FsLoader { roots }, opts)
}

/// Reads `main_file` and resolves it with its imports from `loader`.
pub fn resolve_main(
    sm: &mut SourceMap,
    main_file: &Path,
    loader: &mut dyn Loader,
    opts: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let text = std::fs::read_to_string(main_file).map_err(|e| {
        vec![
            Diagnostic::error(format!("cannot read {}: {e}", main_file.display()))
                .with_code("E1104"),
        ]
    })?;
    let id = sm.add(main_file.display().to_string(), text.clone());
    resolve_program(sm, id, &text, loader, opts)
}

// ---------------------------------------------------------------------
// Loading

struct Loaded {
    file: FileId,
    ast: ast::Module,
    /// Modules whose names are visible unqualified (including the implicit
    /// Prelude).
    imports: Vec<String>,
    /// `import A.B as X`: `X` → `A.B` (visible only as `X.name`).
    aliases: HashMap<String, String>,
    /// Modules usable qualified by their own path (imported without alias).
    qualified: Vec<String>,
    /// `import A.B (x, y)`: module → the names visible unqualified.
    only: Vec<(String, Vec<(String, Span)>)>,
}

struct State {
    loaded: HashMap<String, Loaded>,
    /// Dependency order.
    order: Vec<String>,
    errs: Vec<Diagnostic>,
    visiting: Vec<String>,
    tests: TestMode,
    /// Per module: its tests and the names defined only for tests.
    test_decls: HashMap<String, cirkalang_syntax::testing::Expanded>,
}

impl State {
    #[allow(clippy::too_many_arguments)]
    fn add_module(
        &mut self,
        sm: &mut SourceMap,
        loader: &mut dyn Loader,
        name: String,
        file: FileId,
        mut m: ast::Module,
        prelude: bool,
        expected: Option<Span>,
    ) {
        if let Some((p, sp)) = &m.name
            && p.join(".") != name
        {
            self.errs.push(
                Diagnostic::error(format!(
                    "module `{}` is declared in the file for `{name}`",
                    p.join(".")
                ))
                .with_code("E1104")
                .with_label(*sp, format!("expected `module {name}`")),
            );
        }
        let _ = expected;
        if self.tests == TestMode::Strip {
            cirkalang_syntax::testing::strip(&mut m);
        } else {
            let mut ex = cirkalang_syntax::testing::expand(&mut m);
            self.errs.append(&mut ex.errors);
            self.test_decls.insert(name.clone(), ex);
        }
        self.errs
            .append(&mut cirkalang_syntax::derive::expand(&mut m));
        let mut imports: Vec<(String, Span)> = m
            .imports
            .iter()
            .map(|i| (i.path.join("."), i.span))
            .collect();
        if prelude && name != "Prelude" && !imports.iter().any(|(n, _)| n == "Prelude") {
            imports.insert(0, ("Prelude".to_string(), Span::new(file, 0, 0)));
        }
        // Aliases (spec-v0.0.2.md §7.1): one module per alias, and an alias
        // is not the name of a module imported without one.
        let mut aliases: HashMap<String, String> = HashMap::new();
        for i in &m.imports {
            let Some((a, asp)) = &i.alias else { continue };
            let (a, target) = (a.join("."), i.path.join("."));
            let plain = m
                .imports
                .iter()
                .any(|j| j.alias.is_none() && j.path.join(".") == a);
            match aliases.get(&a) {
                Some(t) if *t != target => self.errs.push(
                    Diagnostic::error(format!("`{a}` is the alias of two modules"))
                        .with_code("E1120")
                        .with_label(*asp, format!("already the alias of `{t}`")),
                ),
                _ if plain || a == name => self.errs.push(
                    Diagnostic::error(format!("the alias `{a}` is the name of a module"))
                        .with_code("E1120")
                        .with_label(*asp, "choose another alias"),
                ),
                _ => {
                    aliases.insert(a, target);
                }
            }
        }
        let unqualified: Vec<String> = imports
            .iter()
            .filter(|(n, _)| {
                n == "Prelude" && !m.imports.iter().any(|i| i.path.join(".") == "Prelude")
                    || m.imports
                        .iter()
                        .any(|i| i.alias.is_none() && i.names.is_none() && i.path.join(".") == *n)
            })
            .map(|(n, _)| n.clone())
            .collect();
        let qualified: Vec<String> = imports
            .iter()
            .filter(|(n, _)| {
                n == "Prelude"
                    || m.imports
                        .iter()
                        .any(|i| i.alias.is_none() && i.path.join(".") == *n)
            })
            .map(|(n, _)| n.clone())
            .collect();
        let only: Vec<(String, Vec<(String, Span)>)> = m
            .imports
            .iter()
            .filter_map(|i| Some((i.path.join("."), i.names.clone()?)))
            .collect();
        self.visiting.push(name.clone());
        for (imp, sp) in &imports {
            if self.visiting.contains(imp) {
                let cycle = self.visiting[self.visiting.iter().position(|v| v == imp).unwrap()..]
                    .join(" -> ");
                self.errs.push(
                    Diagnostic::error(format!("import cycle: {cycle} -> {imp}"))
                        .with_code("E1105")
                        .with_label(*sp, "this import closes the cycle"),
                );
                continue;
            }
            let path: Vec<String> = imp.split('.').map(|s| s.to_string()).collect();
            if let Some(why) = loader.forbidden(&path, &name) {
                self.errs.push(
                    Diagnostic::error(why)
                        .with_code("E1119")
                        .with_label(*sp, "add the package to `[dependencies]`"),
                );
                continue;
            }
            if self.loaded.contains_key(imp) {
                continue;
            }
            match loader.load(sm, &path) {
                Some((fid, text)) => {
                    let (sub, errs) = cirkalang_syntax::parse(fid, &text);
                    self.errs.extend(errs);
                    self.add_module(sm, loader, imp.clone(), fid, sub, prelude, Some(*sp));
                }
                None => {
                    let mut d = Diagnostic::error(format!("module `{imp}` not found"))
                        .with_code("E1104")
                        .with_label(
                            *sp,
                            format!("no file `{}.cirka` in the source roots", path.join("/")),
                        );
                    if let Some(h) = loader.not_found_hint(&path) {
                        d = d.with_help(h);
                    }
                    self.errs.push(d);
                }
            }
        }
        self.visiting.pop();
        self.loaded.insert(
            name.clone(),
            Loaded {
                file,
                ast: m,
                imports: unqualified,
                aliases,
                qualified,
                only,
            },
        );
        self.order.push(name);
    }
}

// ---------------------------------------------------------------------
// Resolution

#[derive(Default)]
struct Table {
    /// Names defined in the module.
    own: HashMap<String, GName>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fixity {
    assoc: ast::Assoc,
    prec: i32,
}

const DEFAULT_FIXITY: Fixity = Fixity {
    assoc: ast::Assoc::Left,
    prec: 9,
};

struct Resolver<'s> {
    st: &'s State,
    globals: Vec<GlobalInfo>,
    tables: HashMap<String, Table>,
    fixity: HashMap<String, (Fixity, Span)>,
    errs: Vec<Diagnostic>,
    // Per-module state.
    cur: String,
    locals: Vec<String>,
    /// Collect unbound lower-case names instead of reporting them.
    auto_bind: Option<Vec<(String, Span)>>,
    fresh: u32,
    /// Per trait: its methods (name, selector).
    traits: HashMap<GName, Vec<(String, GName)>>,
}

impl<'s> Resolver<'s> {
    /// E1113: a declaration outside the tests uses a name defined only for
    /// tests (it would not exist in normal builds).
    fn check_test_only(&mut self, prog: &Program) {
        for m in &prog.modules {
            let Some(ex) = self.st.test_decls.get(&m.name) else {
                continue;
            };
            for it in &m.items {
                let Item::Def(d) = it else { continue };
                if ex.test_only.contains(&self.globals[d.name.0 as usize].name) {
                    continue;
                }
                let mut uses = Vec::new();
                global_uses(&d.sig, &mut uses);
                for c in &d.clauses {
                    global_uses(&c.rhs, &mut uses);
                }
                for (g, sp) in uses {
                    let gi = &self.globals[g.0 as usize];
                    if gi.module == m.name && ex.test_only.contains(&gi.name) {
                        let name = gi.name.clone();
                        self.errs.push(
                            Diagnostic::error(format!(
                                "`{name}` is defined only for tests"
                            ))
                            .with_code("E1113")
                            .with_label(sp, "used outside the tests here")
                            .with_help(format!(
                                "move `{name}` out of the `tests` block, or this use into a test"
                            )),
                        );
                    }
                }
            }
        }
    }

    /// The tests of the modules in `order`.
    fn tests(&self, order: &[String]) -> Vec<Test> {
        let mut out = Vec::new();
        for m in order {
            let Some(ex) = self.st.test_decls.get(m) else {
                continue;
            };
            for t in &ex.tests {
                if let Some(&g) = self.tables[m].own.get(&t.def) {
                    out.push(Test {
                        module: m.clone(),
                        name: t.name.clone(),
                        def: g,
                        span: t.span,
                    });
                }
            }
        }
        out
    }

    /// Replaces `main` by the test main of the root module:
    ///
    /// ```text
    /// main$test = do n <- argCount
    ///                if n == 0 then exit 2 else do
    ///                  a <- getArg 0
    ///                  if strEq a "--list" then do { putStrLn "M\tname"; ...; pure () }
    ///                  else if strEq a "0" then test$0 else ... else exit 2
    /// ```
    fn test_main(&mut self, prog: &mut Program, root: &str) {
        let sp = Span::new(self.st.loaded[root].file, 0, 0);
        let prelude = |n: &str| prog.find("Prelude", n);
        let names = [
            "argCount", "getArg", "strEq", "putStrLn", "pure", "exit", "==", "IO",
        ];
        let gs: Vec<Option<GName>> = names.iter().map(|n| prelude(n)).collect();
        let Some(gs) = gs.into_iter().collect::<Option<Vec<GName>>>() else {
            self.errs.push(
                Diagnostic::error("running tests needs the Prelude")
                    .with_code("E1114")
                    .with_label(sp, "the Prelude was not found or not imported"),
            );
            return;
        };
        let [arg_count, get_arg, str_eq, put_str_ln, pure, exit, eq, io] = gs[..] else {
            unreachable!()
        };
        let e = |k: ExprKind| Expr { kind: k, span: sp };
        let g = |n: GName| e(ExprKind::Global(n));
        let app = |f: Expr, args: Vec<Expr>| {
            args.into_iter()
                .fold(f, |f, a| e(ExprKind::App(Box::new(f), Box::new(a))))
        };
        let str_lit = |s: String| e(ExprKind::Lit(Lit::Str(s)));
        let int = |n: u128| e(ExprKind::Lit(Lit::Int(n)));
        let local = |s: &str| e(ExprKind::Local(s.to_string()));
        let io_unit = || app(g(io), vec![e(ExprKind::Unit)]);
        // `(exit 2 : IO ())`: fixes the result type of `exit`.
        let fail = e(ExprKind::Ann(
            Box::new(app(g(exit), vec![int(2)])),
            Box::new(io_unit()),
        ));
        let mut list: Vec<Stmt> = prog
            .tests
            .iter()
            .map(|t| {
                Stmt::Expr(app(
                    g(put_str_ln),
                    vec![str_lit(format!("{}\t{}", t.module, t.name))],
                ))
            })
            .collect();
        list.push(Stmt::Expr(app(g(pure), vec![e(ExprKind::Unit)])));
        let mut run = fail.clone();
        for (i, t) in prog.tests.iter().enumerate().rev() {
            run = e(ExprKind::If(
                Box::new(app(g(str_eq), vec![local("a"), str_lit(i.to_string())])),
                Box::new(g(t.def)),
                Box::new(run),
            ));
        }
        let run = e(ExprKind::If(
            Box::new(app(
                g(str_eq),
                vec![local("a"), str_lit("--list".to_string())],
            )),
            Box::new(e(ExprKind::Do(list))),
            Box::new(run),
        ));
        let body = e(ExprKind::Do(vec![
            Stmt::Bind("n".into(), sp, g(arg_count)),
            Stmt::Expr(e(ExprKind::If(
                Box::new(app(g(eq), vec![local("n"), int(0)])),
                Box::new(fail),
                Box::new(e(ExprKind::Do(vec![
                    Stmt::Bind("a".into(), sp, app(g(get_arg), vec![int(0)])),
                    Stmt::Expr(run),
                ]))),
            ))),
        ]));
        let name = self.declare(root, "main$test", GKind::Def, sp, true);
        let def = Def {
            name,
            sig: io_unit(),
            clauses: vec![Clause {
                pats: Vec::new(),
                rhs: body,
                span: sp,
                measure: None,
            }],
            span: sp,
            totality: None,
        };
        if let Some(m) = prog.modules.iter_mut().find(|m| m.name == root) {
            m.items.push(Item::Def(def));
        }
        prog.globals = self.globals.clone();
        prog.main = Some(name);
    }

    fn new(st: &'s State) -> Resolver<'s> {
        Resolver {
            st,
            globals: Vec::new(),
            tables: HashMap::new(),
            fixity: HashMap::new(),
            errs: Vec::new(),
            cur: String::new(),
            locals: Vec::new(),
            auto_bind: None,
            fresh: 0,
            traits: HashMap::new(),
        }
    }

    fn declare(
        &mut self,
        module: &str,
        name: &str,
        kind: GKind,
        span: Span,
        private: bool,
    ) -> GName {
        let t = self.tables.entry(module.to_string()).or_default();
        if let Some(&g) = t.own.get(name) {
            let prev = self.globals[g.0 as usize].span;
            self.errs.push(
                Diagnostic::error(format!("`{name}` is defined more than once"))
                    .with_code("E1103")
                    .with_label(span, "redefined here")
                    .with_secondary(prev, "first defined here"),
            );
            return g;
        }
        let g = GName(self.globals.len() as u32);
        self.globals.push(GlobalInfo {
            module: module.to_string(),
            name: name.to_string(),
            kind,
            span,
            private,
        });
        self.tables
            .get_mut(module)
            .unwrap()
            .own
            .insert(name.to_string(), g);
        g
    }

    fn declare_all(&mut self) {
        for name in &self.st.order {
            let m = &self.st.loaded[name].ast;
            self.tables.entry(name.clone()).or_default();
            let mut seen_defs: HashSet<String> = HashSet::new();
            for d in &m.decls {
                match &d.kind {
                    ast::DeclKind::Sig { names, .. } => {
                        for (n, sp) in names {
                            if seen_defs.insert(n.clone()) {
                                self.declare(name, n, GKind::Def, *sp, d.private);
                            } else {
                                self.errs.push(
                                    Diagnostic::error(format!(
                                        "`{n}` has more than one type signature"
                                    ))
                                    .with_code("E1103")
                                    .with_label(*sp, "second signature"),
                                );
                            }
                        }
                    }
                    ast::DeclKind::Clause {
                        name: n, name_span, ..
                    } => {
                        if !seen_defs.contains(n) {
                            self.errs.push(
                                Diagnostic::error(format!("`{n}` has no type signature"))
                                    .with_code("E1107")
                                    .with_label(
                                        *name_span,
                                        "top-level definitions need a signature",
                                    )
                                    .with_help(format!("add `{n} : <type>` before the definition")),
                            );
                            seen_defs.insert(n.clone());
                            self.declare(name, n, GKind::Def, *name_span, d.private);
                        }
                    }
                    ast::DeclKind::Data(dd) => {
                        self.declare(name, &dd.name, GKind::Data, dd.name_span, d.private);
                        for c in &dd.cons {
                            self.declare(name, &c.name, GKind::Con, c.span, d.private);
                        }
                    }
                    ast::DeclKind::Prim {
                        name: n, name_span, ..
                    } => {
                        self.declare(name, n, GKind::Prim, *name_span, d.private);
                    }
                    ast::DeclKind::Extern(b) => {
                        for sig in &b.sigs {
                            self.declare(name, &sig.name, GKind::Prim, sig.name_span, d.private);
                        }
                    }
                    ast::DeclKind::Trait(t) => self.declare_trait(name, t, d.private),
                    // Tests are removed or expanded when the module is
                    // loaded; instances are declared when they are resolved.
                    ast::DeclKind::Fixity { .. }
                    | ast::DeclKind::Test { .. }
                    | ast::DeclKind::Tests(_)
                    | ast::DeclKind::Impl(_) => {}
                }
            }
        }
    }

    fn declare_trait(&mut self, module: &str, t: &ast::TraitDecl, private: bool) {
        let g = self.declare(module, &t.name, GKind::Data, t.name_span, private);
        let con = format!("{}$Dict", t.name);
        self.declare(module, &con, GKind::Con, t.name_span, true);
        for (k, c) in t.context.iter().enumerate() {
            let sel = format!("{}$super{k}", t.name);
            self.declare(module, &sel, GKind::Def, c.span, true);
        }
        let mut methods: Vec<(String, GName)> = Vec::new();
        for it in &t.items {
            if let ast::DeclKind::Sig { names, .. } = &it.kind {
                for (n, sp) in names {
                    let m = self.declare(module, n, GKind::Def, *sp, private);
                    methods.push((n.clone(), m));
                }
            }
        }
        let mut with_default: HashSet<&str> = HashSet::new();
        for it in &t.items {
            let ast::DeclKind::Clause {
                name: n, name_span, ..
            } = &it.kind
            else {
                continue;
            };
            if !methods.iter().any(|(m, _)| m == n) {
                self.errs.push(
                    Diagnostic::error(format!("`{n}` is not a method of trait `{}`", t.name))
                        .with_code("E1115")
                        .with_label(*name_span, "add a signature for it to the trait"),
                );
            } else if with_default.insert(n) {
                let dm = format!("{}$dm${n}", t.name);
                self.declare(module, &dm, GKind::Def, *name_span, true);
            }
        }
        self.traits.insert(g, methods);
    }

    fn collect_fixities(&mut self) {
        for name in &self.st.order {
            for d in &self.st.loaded[name].ast.decls {
                if let ast::DeclKind::Fixity { assoc, prec, ops } = &d.kind {
                    for (op, sp) in ops {
                        let f = Fixity {
                            assoc: *assoc,
                            prec: *prec as i32,
                        };
                        match self.fixity.get(op) {
                            Some((g, prev)) if *g != f => {
                                let prev = *prev;
                                self.errs.push(
                                    Diagnostic::error(format!(
                                        "conflicting fixity declarations for `{op}`"
                                    ))
                                    .with_code("E1110")
                                    .with_label(*sp, "this declaration")
                                    .with_secondary(prev, "conflicts with this one"),
                                );
                            }
                            Some(_) => {}
                            None => {
                                self.fixity.insert(op.clone(), (f, *sp));
                            }
                        }
                    }
                }
            }
        }
    }

    fn fixity_of(&self, name: &str) -> Fixity {
        self.fixity.get(name).map(|x| x.0).unwrap_or(DEFAULT_FIXITY)
    }

    // ---- lookup ----

    fn visible(&self, module: &str, name: &str) -> Option<GName> {
        let g = *self.tables.get(module)?.own.get(name)?;
        if module != self.cur && self.globals[g.0 as usize].private {
            return None;
        }
        Some(g)
    }

    fn lookup_global(&mut self, q: &ast::QName, span: Span) -> Option<GName> {
        if !q.module.is_empty() {
            let mut m = q.module.join(".");
            if let Some(t) = self.st.loaded[&self.cur].aliases.get(&m) {
                m = t.clone();
            }
            let imported = m == self.cur
                || self.st.loaded[&self.cur].qualified.contains(&m)
                || self.st.loaded[&self.cur].aliases.values().any(|t| *t == m)
                    && q.module.join(".") != m;
            if !imported {
                self.errs.push(
                    Diagnostic::error(format!("module `{m}` is not imported"))
                        .with_code("E1101")
                        .with_label(span, format!("add `import {m}`")),
                );
                return None;
            }
            if let Some(g) = self.visible(&m, &q.name) {
                return Some(g);
            }
            let private = self
                .tables
                .get(&m)
                .and_then(|t| t.own.get(&q.name))
                .is_some();
            self.errs.push(
                Diagnostic::error(if private {
                    format!("`{q}` is private")
                } else {
                    format!("`{}` is not defined in module `{m}`", q.name)
                })
                .with_code(if private { "E1106" } else { "E1101" })
                .with_label(
                    span,
                    if private {
                        format!("declared `private` in `{m}`")
                    } else {
                        "unknown name".to_string()
                    },
                ),
            );
            return None;
        }
        if let Some(g) = self.visible(&self.cur.clone(), &q.name) {
            return Some(g);
        }
        let mut found: Vec<GName> = Vec::new();
        let loaded = &self.st.loaded[&self.cur];
        let mods = loaded.imports.iter().chain(
            loaded
                .only
                .iter()
                .filter(|(_, ns)| ns.iter().any(|(n, _)| *n == q.name))
                .map(|(m, _)| m),
        );
        for imp in mods {
            if let Some(g) = self.visible(imp, &q.name)
                && !found.contains(&g)
            {
                found.push(g);
            }
        }
        match found.as_slice() {
            [g] => Some(*g),
            [] => None,
            _ => {
                let mods: Vec<String> = found
                    .iter()
                    .map(|g| self.globals[g.0 as usize].qualified())
                    .collect();
                self.errs.push(
                    Diagnostic::error(format!("`{}` is ambiguous", q.name))
                        .with_code("E1102")
                        .with_label(span, "refers to more than one definition")
                        .with_note(format!("candidates: {}", mods.join(", ")))
                        .with_help("use a qualified name"),
                );
                Some(found[0])
            }
        }
    }

    fn unknown(&mut self, q: &ast::QName, span: Span) {
        self.errs.push(
            Diagnostic::error(format!("`{q}` is not defined"))
                .with_code("E1101")
                .with_label(span, "not in scope"),
        );
    }

    fn resolve_name(&mut self, q: &ast::QName, span: Span) -> ExprKind {
        if q.module.is_empty() && self.locals.iter().rev().any(|l| *l == q.name) {
            return ExprKind::Local(q.name.clone());
        }
        let before = self.errs.len();
        match self.lookup_global(q, span) {
            Some(g) => ExprKind::Global(g),
            None => {
                if self.errs.len() == before {
                    let lower = q.module.is_empty()
                        && q.name.starts_with(|c: char| c.is_lowercase() || c == '_');
                    if lower && let Some(ab) = &mut self.auto_bind {
                        if !ab.iter().any(|(n, _)| *n == q.name) {
                            ab.push((q.name.clone(), span));
                        }
                        return ExprKind::Local(q.name.clone());
                    }
                    self.unknown(q, span);
                }
                ExprKind::Wildcard
            }
        }
    }

    fn fresh_name(&mut self, base: &str) -> String {
        self.fresh += 1;
        format!("{base}${}", self.fresh)
    }

    // ---- modules ----

    fn module(&mut self, name: &str) -> Module {
        self.cur = name.to_string();
        // The names of import lists exist and are public.
        for (m, ns) in self.st.loaded[name].only.clone() {
            if !self.tables.contains_key(&m) {
                continue;
            }
            for (n, sp) in ns {
                if self.visible(&m, &n).is_none() {
                    let private = self.tables[&m].own.contains_key(&n);
                    self.errs.push(
                        Diagnostic::error(if private {
                            format!("`{m}.{n}` is private")
                        } else {
                            format!("`{n}` is not defined in module `{m}`")
                        })
                        .with_code(if private { "E1106" } else { "E1101" })
                        .with_label(sp, "cannot be imported"),
                    );
                }
            }
        }
        let loaded = &self.st.loaded[name];
        let mut items = Vec::new();
        // Group clauses with their signature.
        let mut defs: Vec<Def> = Vec::new();
        let mut def_index: HashMap<String, usize> = HashMap::new();
        let mut impls = 0;
        let decls = loaded.ast.decls.clone();
        for d in &decls {
            match &d.kind {
                ast::DeclKind::Sig {
                    names,
                    ty,
                    totality,
                } => {
                    let sig = self.signature(ty);
                    for (n, sp) in names {
                        let g = self.tables[name].own[n];
                        if def_index.contains_key(n) {
                            continue;
                        }
                        def_index.insert(n.clone(), defs.len());
                        defs.push(Def {
                            name: g,
                            sig: sig.clone(),
                            clauses: Vec::new(),
                            span: *sp,
                            totality: *totality,
                        });
                    }
                }
                ast::DeclKind::Clause {
                    name: n,
                    pats,
                    rhs,
                    measure,
                    ..
                } => {
                    let c = self.clause(pats, rhs, measure.as_ref(), d.span);
                    match def_index.get(n) {
                        Some(&i) => defs[i].clauses.push(c),
                        None => {
                            // Missing signature (already reported).
                            let g = self.tables[name].own[n];
                            def_index.insert(n.clone(), defs.len());
                            defs.push(Def {
                                name: g,
                                sig: Expr {
                                    kind: ExprKind::Wildcard,
                                    span: d.span,
                                },
                                clauses: vec![c],
                                span: d.span,
                                totality: None,
                            });
                        }
                    }
                }
                ast::DeclKind::Data(dd) => {
                    let data = self.data(dd, d.span);
                    items.push(Item::Data(data));
                }
                ast::DeclKind::Prim { name: n, ty, .. } => {
                    let g = self.tables[name].own[n];
                    let t = self.signature(ty);
                    items.push(Item::Prim(g, t));
                }
                ast::DeclKind::Extern(b) => {
                    for sig in &b.sigs {
                        let g = self.tables[name].own[&sig.name];
                        let t = self.signature(&sig.ty);
                        items.push(Item::Extern(g, t, b.abi));
                    }
                }
                ast::DeclKind::Trait(t) => {
                    let (data, tr) = self.trait_(t, d.span);
                    items.push(Item::Data(data));
                    items.push(Item::Trait(tr));
                }
                ast::DeclKind::Impl(i) => {
                    let k = impls;
                    impls += 1;
                    if let Some(it) = self.impl_(i, k, d.span) {
                        items.push(Item::Impl(it));
                    }
                }
                // Tests are removed or expanded when the module is loaded.
                ast::DeclKind::Fixity { .. }
                | ast::DeclKind::Test { .. }
                | ast::DeclKind::Tests(_) => {}
            }
        }
        for d in defs {
            if d.clauses.is_empty() {
                let n = self.globals[d.name.0 as usize].name.clone();
                self.errs.push(
                    Diagnostic::error(format!("`{n}` has a signature but no definition"))
                        .with_code("E1107")
                        .with_label(d.span, "add equations for this name"),
                );
            }
            items.push(Item::Def(d));
        }
        Module {
            name: name.to_string(),
            file: loaded.file,
            items,
        }
    }

    /// A type signature: unbound lower-case names become implicit
    /// arguments `{x : _}` (multiplicity 0) in front.
    fn signature(&mut self, ty: &ast::Expr) -> Expr {
        let saved = self.auto_bind.replace(Vec::new());
        let t = self.expr(ty);
        let bound = std::mem::replace(&mut self.auto_bind, saved).unwrap_or_default();
        bound.into_iter().rev().fold(t, |acc, (n, sp)| Expr {
            span: acc.span,
            kind: ExprKind::Pi {
                mult: Some(Mult::Zero),
                implicit: true,
                name: Some(n),
                name_span: sp,
                dom: Box::new(Expr {
                    kind: ExprKind::Wildcard,
                    span: sp,
                }),
                cod: Box::new(acc),
            },
        })
    }

    fn data(&mut self, dd: &ast::DataDecl, span: Span) -> Data {
        let name = self.tables[&self.cur].own[&dd.name];
        let mut params = Vec::new();
        let base = self.locals.len();
        for p in &dd.params {
            let ty = p.ty.as_ref().map(|t| self.expr(t));
            for (n, sp) in &p.names {
                params.push(Param {
                    mult: p.mult,
                    name: n.clone(),
                    span: *sp,
                    ty: ty.clone(),
                });
                self.locals.push(n.clone());
            }
        }
        let sig = dd.sig.as_ref().map(|t| self.signature(t));
        let mut cons = Vec::new();
        for c in &dd.cons {
            let g = self.tables[&self.cur].own[&c.name];
            let body = match &c.body {
                ast::ConBody::Sig(t) => ConBody::Sig(self.signature(t)),
                ast::ConBody::Fields(fs) => {
                    ConBody::Fields(fs.iter().map(|f| self.expr(f)).collect())
                }
            };
            cons.push(Con {
                name: g,
                span: c.span,
                body,
            });
        }
        self.locals.truncate(base);
        Data {
            name,
            params,
            sig,
            cons,
            short: dd.short,
            span,
        }
    }

    /// A trait as its dictionary data type plus what elaboration generates.
    fn trait_(&mut self, t: &ast::TraitDecl, span: Span) -> (Data, Trait) {
        let own = |r: &Self, n: &str| r.tables[&r.cur].own[n];
        let name = own(self, &t.name);
        let dict_con = own(self, &format!("{}$Dict", t.name));
        let base = self.locals.len();
        let mut params = Vec::new();
        let mut pnames = Vec::new();
        for p in &t.params {
            let ty = p.ty.as_ref().map(|x| self.expr(x));
            for (n, sp) in &p.names {
                params.push(Param {
                    mult: Some(Mult::Zero),
                    name: n.clone(),
                    span: *sp,
                    ty: ty.clone(),
                });
                pnames.push(n.clone());
                self.locals.push(n.clone());
            }
        }
        let mut fields: Vec<Expr> = Vec::new();
        let mut supers = Vec::new();
        for (k, c) in t.context.iter().enumerate() {
            fields.push(self.expr(c));
            supers.push(own(self, &format!("{}$super{k}", t.name)));
        }
        let mut methods: Vec<TraitMethod> = Vec::new();
        for it in &t.items {
            if let ast::DeclKind::Sig { names, ty, .. } = &it.kind {
                let sig = self.signature(ty);
                for (n, sp) in names {
                    fields.push(sig.clone());
                    methods.push(TraitMethod {
                        name: own(self, n),
                        default: None,
                        span: *sp,
                    });
                }
            }
        }
        for it in &t.items {
            let ast::DeclKind::Clause {
                name: n,
                pats,
                rhs,
                measure,
                ..
            } = &it.kind
            else {
                continue;
            };
            self.no_measure(measure.as_ref());
            let g = self.tables[&self.cur].own.get(n).copied();
            let Some(m) = methods.iter_mut().find(|m| Some(m.name) == g) else {
                continue; // reported by `declare_trait`
            };
            let dm = self.tables[&self.cur].own[&format!("{}$dm${n}", t.name)];
            let c = self.clause(pats, rhs, None, it.span);
            m.default.get_or_insert((dm, Vec::new())).1.push(c);
        }
        self.locals.truncate(base);
        // `Name$Dict : field1 -> ... -> Name params`.
        let e = |kind| Expr { kind, span };
        let mut sig = pnames.iter().fold(e(ExprKind::Global(name)), |f, p| {
            e(ExprKind::App(
                Box::new(f),
                Box::new(e(ExprKind::Local(p.clone()))),
            ))
        });
        for f in fields.into_iter().rev() {
            sig = Expr {
                span: f.span,
                kind: ExprKind::Pi {
                    mult: Some(Mult::Many),
                    implicit: false,
                    name: None,
                    name_span: f.span,
                    dom: Box::new(f),
                    cod: Box::new(sig),
                },
            };
        }
        let data = Data {
            name,
            params,
            sig: None,
            cons: vec![Con {
                name: dict_con,
                span: t.name_span,
                body: ConBody::Sig(sig),
            }],
            short: false,
            span,
        };
        let tr = Trait {
            name,
            dict_con,
            params: pnames,
            supers,
            methods,
            span,
        };
        (data, tr)
    }

    fn impl_(&mut self, i: &ast::ImplDecl, k: usize, span: Span) -> Option<Impl> {
        let kinds: Vec<Expr> = i.binders.iter().map(|(_, _, k)| self.expr(k)).collect();
        let declared: Vec<(String, Span)> = i
            .binders
            .iter()
            .map(|(n, sp, _)| (n.clone(), *sp))
            .collect();
        let saved = self.auto_bind.replace(declared);
        let head = self.expr(&i.head);
        let context: Vec<Expr> = i.context.iter().map(|c| self.expr(c)).collect();
        let vars = std::mem::replace(&mut self.auto_bind, saved).unwrap_or_default();
        let mut h = &head;
        while let ExprKind::App(f, _) = &h.kind {
            h = f;
        }
        let tr = match h.kind {
            ExprKind::Global(g) if self.traits.contains_key(&g) => g,
            _ => {
                if !matches!(h.kind, ExprKind::Wildcard) {
                    self.errs.push(
                        Diagnostic::error("the head of an `impl` must be a trait")
                            .with_code("E1115")
                            .with_label(h.span, "not a trait"),
                    );
                }
                return None;
            }
        };
        let tname = self.globals[tr.0 as usize].name.clone();
        let cur = self.cur.clone();
        let name = self.declare(&cur, &format!("impl${tname}${k}"), GKind::Def, span, true);
        let trait_methods = self.traits[&tr].clone();
        let base = self.locals.len();
        self.locals.extend(vars.iter().map(|v| v.0.clone()));
        let mut methods: Vec<ImplMethod> = trait_methods
            .iter()
            .map(|(n, m)| ImplMethod {
                method: *m,
                def: self.declare(
                    &cur,
                    &format!("impl${tname}${k}${n}"),
                    GKind::Def,
                    span,
                    true,
                ),
                clauses: Vec::new(),
            })
            .collect();
        for it in &i.items {
            let ast::DeclKind::Clause {
                name: n,
                name_span,
                pats,
                rhs,
                measure,
            } = &it.kind
            else {
                continue;
            };
            self.no_measure(measure.as_ref());
            match trait_methods.iter().position(|(m, _)| m == n) {
                Some(j) => {
                    let c = self.clause(pats, rhs, None, it.span);
                    methods[j].clauses.push(c);
                }
                None => self.errs.push(
                    Diagnostic::error(format!("`{n}` is not a method of trait `{tname}`"))
                        .with_code("E1115")
                        .with_label(*name_span, "defined in this `impl`"),
                ),
            }
        }
        self.locals.truncate(base);
        Some(Impl {
            name,
            vars,
            kinds,
            context,
            head,
            trait_: tr,
            methods,
            span,
        })
    }

    fn clause(
        &mut self,
        pats: &[ast::Pat],
        rhs: &ast::Expr,
        measure: Option<&ast::Expr>,
        span: Span,
    ) -> Clause {
        let base = self.locals.len();
        let mut bound = Vec::new();
        let pats: Vec<Pat> = pats.iter().map(|p| self.pat(p, &mut bound)).collect();
        self.locals.extend(bound);
        let rhs = self.expr(rhs);
        let measure = measure.map(|m| self.expr(m));
        self.locals.truncate(base);
        Clause {
            pats,
            rhs,
            span,
            measure,
        }
    }

    /// Measures are only given to top-level functions.
    fn no_measure(&mut self, m: Option<&ast::Expr>) {
        if let Some(m) = m {
            self.errs.push(
                Diagnostic::error("`decreasing` is only supported on top-level functions")
                    .with_code("E1004")
                    .with_label(m.span, "methods cannot have a measure in this version"),
            );
        }
    }

    // ---- patterns ----

    fn pat(&mut self, p: &ast::Pat, bound: &mut Vec<String>) -> Pat {
        let kind = match &p.kind {
            ast::PatKind::Var(x) => {
                if bound.contains(x) {
                    self.errs.push(
                        Diagnostic::error(format!(
                            "`{x}` is bound more than once in the same pattern"
                        ))
                        .with_code("E1109")
                        .with_label(p.span, "rename one of the occurrences"),
                    );
                }
                bound.push(x.clone());
                PatKind::Var(x.clone())
            }
            ast::PatKind::Wild => PatKind::Wild,
            ast::PatKind::Con(q, args) => match self.con_ref(q, p.span) {
                Some(g) => PatKind::Con(g, args.iter().map(|a| self.pat(a, bound)).collect()),
                None => PatKind::Wild,
            },
            ast::PatKind::Lit(l, neg) => PatKind::Lit(l.clone(), *neg),
            ast::PatKind::Unit => PatKind::Unit,
            ast::PatKind::Tuple(ps) => {
                PatKind::Tuple(ps.iter().map(|a| self.pat(a, bound)).collect())
            }
            ast::PatKind::List(ps) => {
                PatKind::List(ps.iter().map(|a| self.pat(a, bound)).collect())
            }
            ast::PatKind::OpSeq(ps, ops) => {
                let ps: Vec<Pat> = ps.iter().map(|a| self.pat(a, bound)).collect();
                return self.assoc_pats(ps, ops, p.span);
            }
        };
        Pat { kind, span: p.span }
    }

    fn con_ref(&mut self, q: &ast::QName, span: Span) -> Option<GName> {
        let before = self.errs.len();
        match self.lookup_global(q, span) {
            Some(g) if self.globals[g.0 as usize].kind == GKind::Con => Some(g),
            Some(_) => {
                self.errs.push(
                    Diagnostic::error(format!("`{q}` is not a constructor"))
                        .with_code("E1108")
                        .with_label(span, "only constructors can be matched"),
                );
                None
            }
            None => {
                if self.errs.len() == before {
                    self.unknown(q, span);
                }
                None
            }
        }
    }

    /// Associates `p1 op p2 op p3` by fixity (constructor operators).
    fn assoc_pats(&mut self, ps: Vec<Pat>, ops: &[ast::OpRef], span: Span) -> Pat {
        let mut items: Vec<Either<Pat, (ast::OpRef, Fixity)>> = Vec::new();
        for (i, p) in ps.into_iter().enumerate() {
            if i > 0 {
                let o = ops[i - 1].clone();
                let f = self.fixity_of(&o.name.name);
                items.push(Either::R((o, f)));
            }
            items.push(Either::L(p));
        }
        let mut it = items.into_iter().peekable();
        match self.assoc(
            &mut it,
            -1,
            ast::Assoc::None,
            None,
            span,
            &mut |r: &mut Self, o: &ast::OpRef, l: Pat, rr: Pat| {
                let sp = l.span.to(rr.span);
                match r.con_ref(&o.name, o.span) {
                    Some(g) => Pat {
                        kind: PatKind::Con(g, vec![l, rr]),
                        span: sp,
                    },
                    None => Pat {
                        kind: PatKind::Wild,
                        span: sp,
                    },
                }
            },
        ) {
            Some(p) => p,
            None => Pat {
                kind: PatKind::Wild,
                span,
            },
        }
    }

    /// Haskell 2010 §10.6 operator resolution over a token stream.
    #[allow(clippy::too_many_arguments)]
    fn assoc<T, I: Iterator<Item = Either<T, (ast::OpRef, Fixity)>>>(
        &mut self,
        it: &mut std::iter::Peekable<I>,
        prec1: i32,
        assoc1: ast::Assoc,
        neg: Option<&mut dyn FnMut(&mut Self, T) -> T>,
        span: Span,
        mk: &mut dyn FnMut(&mut Self, &ast::OpRef, T, T) -> T,
    ) -> Option<T> {
        let _ = neg;
        let first = match it.next()? {
            Either::L(x) => x,
            Either::R(_) => return None,
        };
        self.assoc_rest(it, prec1, assoc1, first, span, mk)
    }

    fn assoc_rest<T, I: Iterator<Item = Either<T, (ast::OpRef, Fixity)>>>(
        &mut self,
        it: &mut std::iter::Peekable<I>,
        prec1: i32,
        assoc1: ast::Assoc,
        mut lhs: T,
        _span: Span,
        mk: &mut dyn FnMut(&mut Self, &ast::OpRef, T, T) -> T,
    ) -> Option<T> {
        loop {
            let (op2, f2) = match it.peek() {
                Some(Either::R((o, f))) => (o.clone(), *f),
                _ => return Some(lhs),
            };
            if prec1 == f2.prec && (assoc1 != f2.assoc || assoc1 == ast::Assoc::None) {
                self.errs.push(
                    Diagnostic::error(format!("cannot mix operators with the same precedence {} but different associativity", f2.prec))
                        .with_code("E1111")
                        .with_label(op2.span, "add parentheses"),
                );
                return None;
            }
            if prec1 > f2.prec || (prec1 == f2.prec && assoc1 == ast::Assoc::Left) {
                return Some(lhs);
            }
            it.next();
            let rhs_first = match it.next()? {
                Either::L(x) => x,
                Either::R(_) => return None,
            };
            let rhs = self.assoc_rest(it, f2.prec, f2.assoc, rhs_first, _span, mk)?;
            lhs = mk(self, &op2, lhs, rhs);
        }
    }

    // ---- expressions ----

    fn expr(&mut self, e: &ast::Expr) -> Expr {
        let span = e.span;
        let kind = match &e.kind {
            ast::ExprKind::Var(q) => self.resolve_name(q, span),
            ast::ExprKind::Lit(l) => ExprKind::Lit(l.clone()),
            ast::ExprKind::Hole(h) => ExprKind::Hole(h.clone()),
            ast::ExprKind::Wildcard => ExprKind::Wildcard,
            ast::ExprKind::Type => ExprKind::Type,
            ast::ExprKind::LType => ExprKind::LType,
            ast::ExprKind::Unit => ExprKind::Unit,
            ast::ExprKind::App(f, a) => {
                ExprKind::App(Box::new(self.expr(f)), Box::new(self.expr(a)))
            }
            ast::ExprKind::Lam(bs, body) => {
                let base = self.locals.len();
                let mut out = Vec::new();
                for b in bs {
                    let ty = b.ty.as_ref().map(|t| self.expr(t));
                    if let Some(n) = &b.name {
                        self.locals.push(n.clone());
                    }
                    out.push(LamBinder {
                        name: b.name.clone(),
                        span: b.span,
                        mult: b.mult,
                        ty,
                    });
                }
                let body = self.expr(body);
                self.locals.truncate(base);
                ExprKind::Lam(out, Box::new(body))
            }
            ast::ExprKind::Let(bs, body) => {
                let base = self.locals.len();
                let bs = self.let_binds(bs);
                let body = self.expr(body);
                self.locals.truncate(base);
                ExprKind::Let(bs, Box::new(body))
            }
            ast::ExprKind::Case(s, alts) => {
                let s = self.expr(s);
                let alts = alts
                    .iter()
                    .map(|a| {
                        let base = self.locals.len();
                        let mut bound = Vec::new();
                        let pat = self.pat(&a.pat, &mut bound);
                        self.locals.extend(bound);
                        let rhs = self.expr(&a.rhs);
                        self.locals.truncate(base);
                        Alt { pat, rhs }
                    })
                    .collect();
                ExprKind::Case(Box::new(s), alts)
            }
            ast::ExprKind::If(c, t, f) => ExprKind::If(
                Box::new(self.expr(c)),
                Box::new(self.expr(t)),
                Box::new(self.expr(f)),
            ),
            ast::ExprKind::Do(stmts) => {
                let base = self.locals.len();
                let mut out = Vec::new();
                for (i, s) in stmts.iter().enumerate() {
                    match s {
                        // `p <- e; rest` = `x <- e; case x of p => do rest`.
                        ast::Stmt::BindPat(p, e, others) => {
                            let x = self.fresh_name("bind");
                            let re = self.expr(e);
                            out.push(Stmt::Bind(x.clone(), p.span, re));
                            let rest = &stmts[i + 1..];
                            let span = e.span;
                            let body = if rest.is_empty() {
                                self.errs.push(
                                    Diagnostic::error(
                                        "the last statement of a `do` block cannot bind a pattern",
                                    )
                                    .with_code("E1002")
                                    .with_label(p.span, "nothing uses this pattern"),
                                );
                                break;
                            } else {
                                ast::Expr {
                                    kind: ast::ExprKind::Do(rest.to_vec()),
                                    span,
                                }
                            };
                            let case = ast::Expr {
                                kind: ast::ExprKind::Case(
                                    Box::new(ast::Expr {
                                        kind: ast::ExprKind::Var(ast::QName::simple(x.clone())),
                                        span,
                                    }),
                                    std::iter::once(ast::Alt {
                                        pat: p.clone(),
                                        rhs: body,
                                    })
                                    .chain(others.iter().cloned())
                                    .collect(),
                                ),
                                span,
                            };
                            self.locals.push(x);
                            out.push(Stmt::Expr(self.expr(&case)));
                            break;
                        }
                        ast::Stmt::Bind(x, sp, e) => {
                            let e = self.expr(e);
                            self.locals.push(x.clone());
                            out.push(Stmt::Bind(x.clone(), *sp, e));
                        }
                        ast::Stmt::Let(bs) => {
                            let bs = self.let_binds(bs);
                            out.push(Stmt::Let(bs));
                        }
                        ast::Stmt::Expr(e) => out.push(Stmt::Expr(self.expr(e))),
                        ast::Stmt::Try(x, e, q) => {
                            let e = self.expr(e);
                            if let Some((x, _)) = x {
                                self.locals.push(x.clone());
                            }
                            out.push(Stmt::Try(x.clone(), e, *q));
                        }
                    }
                }
                self.locals.truncate(base);
                ExprKind::Do(out)
            }
            ast::ExprKind::Pi {
                mult,
                implicit,
                names,
                dom,
                cod,
            } => {
                let dom = self.expr(dom);
                if names.is_empty() {
                    let cod = self.expr(cod);
                    ExprKind::Pi {
                        mult: *mult,
                        implicit: *implicit,
                        name: None,
                        name_span: span,
                        dom: Box::new(dom),
                        cod: Box::new(cod),
                    }
                } else {
                    let base = self.locals.len();
                    for (n, _) in names {
                        self.locals.push(n.clone());
                    }
                    let mut acc = self.expr(cod);
                    self.locals.truncate(base);
                    for (n, sp) in names.iter().rev() {
                        acc = Expr {
                            span,
                            kind: ExprKind::Pi {
                                mult: *mult,
                                implicit: *implicit,
                                name: if n == "_" { None } else { Some(n.clone()) },
                                name_span: *sp,
                                dom: Box::new(dom.clone()),
                                cod: Box::new(acc),
                            },
                        };
                    }
                    return acc;
                }
            }
            ast::ExprKind::Ann(e, t) => {
                ExprKind::Ann(Box::new(self.expr(e)), Box::new(self.expr(t)))
            }
            ast::ExprKind::Tuple(es) => ExprKind::Tuple(es.iter().map(|x| self.expr(x)).collect()),
            ast::ExprKind::List(es) => ExprKind::List(es.iter().map(|x| self.expr(x)).collect()),
            ast::ExprKind::OpSeq { neg, operands, ops } => {
                return self.opseq(*neg, operands, ops, span);
            }
            ast::ExprKind::LeftSection(e, o) => {
                let f = self.op_expr(o);
                let e = self.expr(e);
                ExprKind::App(Box::new(f), Box::new(e))
            }
            ast::ExprKind::RightSection(o, e) => {
                let x = self.fresh_name("sec");
                let f = self.op_expr(o);
                let e = self.expr(e);
                let xv = Expr {
                    kind: ExprKind::Local(x.clone()),
                    span: o.span,
                };
                let body = app2(f, xv, e, span);
                ExprKind::Lam(
                    vec![LamBinder {
                        name: Some(x),
                        span: o.span,
                        mult: None,
                        ty: None,
                    }],
                    Box::new(body),
                )
            }
        };
        Expr { kind, span }
    }

    fn let_binds(&mut self, bs: &[ast::LetBind]) -> Vec<LetBind> {
        let mut out = Vec::new();
        for b in bs {
            let ty = b.ty.as_ref().map(|t| self.expr(t));
            let rhs = if b.args.is_empty() {
                self.expr(&b.rhs)
            } else {
                let base = self.locals.len();
                let mut binders = Vec::new();
                for a in &b.args {
                    match &a.kind {
                        ast::PatKind::Var(x) => {
                            self.locals.push(x.clone());
                            binders.push(LamBinder {
                                name: Some(x.clone()),
                                span: a.span,
                                mult: None,
                                ty: None,
                            });
                        }
                        ast::PatKind::Wild => binders.push(LamBinder {
                            name: None,
                            span: a.span,
                            mult: None,
                            ty: None,
                        }),
                        _ => self.errs.push(
                            Diagnostic::error("arguments of a local function must be variables")
                                .with_code("E1108")
                                .with_label(a.span, "use `case` to match on this argument"),
                        ),
                    }
                }
                let body = self.expr(&b.rhs);
                self.locals.truncate(base);
                Expr {
                    span: b.rhs.span,
                    kind: ExprKind::Lam(binders, Box::new(body)),
                }
            };
            // Bindings are sequential: later ones see earlier ones.
            self.locals.push(b.name.clone());
            out.push(LetBind {
                mult: b.mult,
                name: b.name.clone(),
                span: b.name_span,
                ty,
                rhs,
            });
        }
        out
    }

    fn op_expr(&mut self, o: &ast::OpRef) -> Expr {
        Expr {
            kind: self.resolve_name(&o.name, o.span),
            span: o.span,
        }
    }

    fn opseq(
        &mut self,
        neg: Option<Span>,
        operands: &[ast::Expr],
        ops: &[ast::OpRef],
        span: Span,
    ) -> Expr {
        let mut items: Vec<Either<Expr, (ast::OpRef, Fixity)>> = Vec::new();
        for (i, e) in operands.iter().enumerate() {
            if i > 0 {
                let o = ops[i - 1].clone();
                let f = self.fixity_of(&o.name.name);
                items.push(Either::R((o, f)));
            }
            items.push(Either::L(self.expr(e)));
        }
        let mut mk = |r: &mut Self, o: &ast::OpRef, l: Expr, rr: Expr| {
            let f = r.op_expr(o);
            let sp = l.span.to(rr.span);
            app2(f, l, rr, sp)
        };
        let mut it = items.into_iter().peekable();
        let result = if let Some(nsp) = neg {
            // `- e1 op ...`: negation binds like a precedence-6 left operator.
            let first = match it.next() {
                Some(Either::L(x)) => x,
                _ => return err_expr(span),
            };
            let operand = match self.assoc_rest(&mut it, 6, ast::Assoc::Left, first, span, &mut mk)
            {
                Some(x) => x,
                None => return err_expr(span),
            };
            let negate = Expr {
                kind: self.resolve_name(&ast::QName::simple("negate"), nsp),
                span: nsp,
            };
            let sp = nsp.to(operand.span);
            let n = Expr {
                kind: ExprKind::App(Box::new(negate), Box::new(operand)),
                span: sp,
            };
            self.assoc_rest(&mut it, -1, ast::Assoc::None, n, span, &mut mk)
        } else {
            self.assoc(&mut it, -1, ast::Assoc::None, None, span, &mut mk)
        };
        result.unwrap_or_else(|| err_expr(span))
    }
}

enum Either<A, B> {
    L(A),
    R(B),
}

fn app2(f: Expr, a: Expr, b: Expr, span: Span) -> Expr {
    let fa = Expr {
        span,
        kind: ExprKind::App(Box::new(f), Box::new(a)),
    };
    Expr {
        span,
        kind: ExprKind::App(Box::new(fa), Box::new(b)),
    }
}

fn err_expr(span: Span) -> Expr {
    Expr {
        kind: ExprKind::Wildcard,
        span,
    }
}

pub mod print;

#[cfg(test)]
mod tests;

/// The globals used in `e`, with the spans of the uses.
fn global_uses(e: &Expr, out: &mut Vec<(GName, Span)>) {
    let mut go = |x: &Expr| global_uses(x, out);
    match &e.kind {
        ExprKind::Global(g) => out.push((*g, e.span)),
        ExprKind::Local(_)
        | ExprKind::Lit(_)
        | ExprKind::Hole(_)
        | ExprKind::Wildcard
        | ExprKind::Type
        | ExprKind::LType
        | ExprKind::Unit => {}
        ExprKind::App(f, a) => {
            go(f);
            go(a);
        }
        ExprKind::Lam(bs, b) => {
            for t in bs.iter().filter_map(|b| b.ty.as_ref()) {
                go(t);
            }
            go(b);
        }
        ExprKind::Let(bs, b) => {
            for lb in bs {
                if let Some(t) = &lb.ty {
                    go(t);
                }
                go(&lb.rhs);
            }
            go(b);
        }
        ExprKind::Case(s, alts) => {
            go(s);
            for a in alts {
                go(&a.rhs);
            }
        }
        ExprKind::If(c, t, f) => {
            go(c);
            go(t);
            go(f);
        }
        ExprKind::Do(ss) => {
            for s in ss {
                match s {
                    Stmt::Bind(_, _, x) | Stmt::Expr(x) | Stmt::Try(_, x, _) => go(x),
                    Stmt::Let(bs) => {
                        for lb in bs {
                            if let Some(t) = &lb.ty {
                                go(t);
                            }
                            go(&lb.rhs);
                        }
                    }
                }
            }
        }
        ExprKind::Pi { dom, cod, .. } => {
            go(dom);
            go(cod);
        }
        ExprKind::Ann(x, t) => {
            go(x);
            go(t);
        }
        ExprKind::Tuple(xs) | ExprKind::List(xs) => {
            for x in xs {
                go(x);
            }
        }
    }
}
