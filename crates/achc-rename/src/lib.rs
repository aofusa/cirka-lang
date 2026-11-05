//! achc-rename: module graph, import/export, scopes and fixity resolution
//! for aCHC (docs/design/01-architecture.md).
//!
//! The output is the syntax AST with every name resolved:
//! - a global name is a `QName` whose `qual` is its defining module;
//! - a local name has no qualifier and a unique name `x#N`;
//! - operator sequences are re-associated by fixity into applications
//!   (prefix `-` becomes `Prelude.negate`), and parentheses are removed.
//!
//! The built-in syntax (`[]`, `:`, `()`, tuples, `->`) belongs to module
//! `Cirka.Prim`.

mod fixity;
mod rename;
mod scope;

use achc_syntax::ast;
use cirka_diagnostics::{Diagnostic, FileId, SourceMap, Span};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub use rename::{BASE_MODULE, display_name};

/// The module of the built-in syntax and primitives.
pub const PRIM: &str = "Cirka.Prim";

/// Finds and loads the source of a module.
pub trait Loader {
    /// Loads module `name` (e.g. `Data.List`), adding it to `sm`. Returns
    /// the file, its text and whether it is a `.chs` file.
    fn load(&mut self, sm: &mut SourceMap, name: &str) -> Option<(FileId, String, bool)>;
}

/// Loads `A/B.hs` (or `A/B.chs`) from the first root that has it.
pub struct FsLoader {
    pub roots: Vec<PathBuf>,
}

impl Loader for FsLoader {
    fn load(&mut self, sm: &mut SourceMap, name: &str) -> Option<(FileId, String, bool)> {
        let rel: PathBuf = name.split('.').collect();
        for r in &self.roots {
            for (ext, chs) in [("hs", false), ("chs", true)] {
                let p = r.join(&rel).with_extension(ext);
                if let Ok(src) = std::fs::read_to_string(&p) {
                    let id = sm.add(p.display().to_string(), src.clone());
                    return Some((id, src, chs));
                }
            }
        }
        None
    }
}

/// Modules held in memory (tests).
pub struct MemLoader(pub HashMap<String, String>);

impl Loader for MemLoader {
    fn load(&mut self, sm: &mut SourceMap, name: &str) -> Option<(FileId, String, bool)> {
        let src = self.0.get(name)?.clone();
        let id = sm.add(format!("{name}.hs"), src.clone());
        Some((id, src, false))
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Import `Prelude` implicitly (unless `NoImplicitPrelude`).
    pub implicit_prelude: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            implicit_prelude: true,
        }
    }
}

/// A renamed module.
#[derive(Clone, Debug)]
pub struct RnModule {
    pub name: String,
    pub file: FileId,
    /// A `.chs` file (Cirka mode).
    pub chs: bool,
    pub extensions: Vec<String>,
    pub ast: ast::Module,
}

/// The renamed program: modules in dependency order (the main module
/// last), with the fixities of all operators.
#[derive(Clone, Debug)]
pub struct Program {
    pub modules: Vec<RnModule>,
}

/// Extensions accepted in `{-# LANGUAGE #-}` (02-language-extensions.md
/// §1); anything else is E0001.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "Haskell2010",
    "Haskell98",
    "NoImplicitPrelude",
    "BangPatterns",
    "ScopedTypeVariables",
    "LambdaCase",
    "TupleSections",
    "MultiWayIf",
    "RankNTypes",
    "Rank2Types",
    "MultiParamTypeClasses",
    "FlexibleInstances",
    "FlexibleContexts",
    "GeneralizedNewtypeDeriving",
    "KindSignatures",
    "LinearTypes",
    "Strict",
    "StrictData",
    "EmptyDataDecls",
    "CirkaOwnership",
    "CirkaUnique",
    "CirkaDrop",
    "CirkaBorrow",
    "CirkaRegions",
    "CirkaExtern",
];

pub(crate) fn err(code: &str, span: Span, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg).with_code(code).with_label(span, "")
}

struct Parsed {
    name: String,
    file: FileId,
    chs: bool,
    ast: ast::Module,
}

/// Loads the main module and everything it imports, then renames them.
pub fn rename_program(
    sm: &mut SourceMap,
    main: FileId,
    src: &str,
    chs: bool,
    loader: &mut dyn Loader,
    opts: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let mut errs = Vec::new();
    let mut parsed: HashMap<String, Parsed> = HashMap::new();
    let mut order_hint = Vec::new();
    let main_mod = parse_one(main, src, chs, opts, &mut errs);
    let main_name = main_mod.name.clone();
    let mut queue = vec![main_mod];
    while let Some(m) = queue.pop() {
        for imp in &m.ast.imports {
            let n = &imp.module.name;
            if parsed.contains_key(n) || queue.iter().any(|q| &q.name == n) || n == &m.name {
                continue;
            }
            match loader.load(sm, n) {
                Some((id, text, chs)) => {
                    let pm = parse_one(id, &text, chs, opts, &mut errs);
                    if pm.name != *n {
                        errs.push(err(
                            "E0201",
                            pm.ast.name.as_ref().map(|i| i.span).unwrap_or(pm.ast.span),
                            format!("file for module `{n}` declares module `{}`", pm.name),
                        ));
                    }
                    queue.push(pm);
                }
                None => errs.push(
                    err("E0201", imp.module.span, format!("module `{n}` not found"))
                        .with_help("check the module name and the library search path"),
                ),
            }
        }
        order_hint.push(m.name.clone());
        parsed.insert(m.name.clone(), m);
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    // Topological order of the import graph.
    let mut order = Vec::new();
    let mut state: HashMap<String, u8> = HashMap::new();
    fn visit(
        n: &str,
        parsed: &HashMap<String, Parsed>,
        state: &mut HashMap<String, u8>,
        order: &mut Vec<String>,
        stack: &mut Vec<String>,
        errs: &mut Vec<Diagnostic>,
    ) {
        match state.get(n) {
            Some(2) => return,
            Some(1) => {
                let m = &parsed[n];
                let mut cycle = stack.clone();
                cycle.push(n.to_string());
                errs.push(
                    err(
                        "E0201",
                        m.ast.name.as_ref().map(|i| i.span).unwrap_or(m.ast.span),
                        format!("import cycle: {}", cycle.join(" -> ")),
                    )
                    .with_note("mutually recursive modules are not supported"),
                );
                return;
            }
            _ => {}
        }
        state.insert(n.to_string(), 1);
        stack.push(n.to_string());
        let m = &parsed[n];
        for imp in &m.ast.imports {
            if parsed.contains_key(&imp.module.name) && imp.module.name != n {
                visit(&imp.module.name, parsed, state, order, stack, errs);
            }
        }
        stack.pop();
        state.insert(n.to_string(), 2);
        order.push(n.to_string());
    }
    visit(
        &main_name,
        &parsed,
        &mut state,
        &mut order,
        &mut Vec::new(),
        &mut errs,
    );
    if !errs.is_empty() {
        return Err(errs);
    }
    let mut r = rename::Renamer::new();
    let mut modules = Vec::new();
    for n in &order {
        let p = parsed.remove(n).unwrap();
        let exts: Vec<String> = p.ast.extensions.iter().map(|e| e.0.clone()).collect();
        let ast = r.module(&p.name, p.ast);
        modules.push(RnModule {
            name: p.name,
            file: p.file,
            chs: p.chs,
            extensions: exts,
            ast,
        });
    }
    let _ = order_hint;
    if !r.errors.is_empty() {
        return Err(r.errors);
    }
    Ok(Program { modules })
}

fn parse_one(
    file: FileId,
    src: &str,
    chs: bool,
    opts: &Options,
    errs: &mut Vec<Diagnostic>,
) -> Parsed {
    let (mut m, perrs) = achc_syntax::parse(file, src);
    errs.extend(perrs);
    let mut seen = HashSet::new();
    for (e, sp) in &m.extensions {
        if !SUPPORTED_EXTENSIONS.contains(&e.as_str()) && seen.insert(e.clone()) {
            errs.push(
                err("E0001", *sp, format!("unsupported extension `{e}`")).with_note(
                    "see docs/design/02-language-extensions.md §1.2 for the supported extensions",
                ),
            );
        }
    }
    let name = m
        .name
        .as_ref()
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "Main".into());
    let no_prelude = m.extensions.iter().any(|e| e.0 == "NoImplicitPrelude");
    let explicit = m.imports.iter().any(|i| i.module.name == "Prelude");
    if opts.implicit_prelude && !no_prelude && !explicit && name != "Prelude" {
        let sp = Span::new(file, 0, 0);
        m.imports.push(ast::Import {
            module: ast::Ident {
                name: "Prelude".into(),
                span: sp,
            },
            qualified: false,
            alias: None,
            hiding: false,
            items: None,
            span: sp,
        });
    }
    Parsed {
        name,
        file,
        chs,
        ast: m,
    }
}

/// Renames a single module whose imports are given in memory (tests).
pub fn rename_source(
    sm: &mut SourceMap,
    src: &str,
    modules: &[(&str, &str)],
    opts: &Options,
) -> Result<Program, Vec<Diagnostic>> {
    let id = sm.add("Main.hs", src.to_string());
    let mut l = MemLoader(
        modules
            .iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect(),
    );
    rename_program(sm, id, src, false, &mut l, opts)
}

#[cfg(test)]
mod tests;
