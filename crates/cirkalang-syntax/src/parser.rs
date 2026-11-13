//! Recursive-descent parser (spec-v0.0.1.md §3).

use crate::ast::*;
use crate::lexer::{Kw, Tok, Token};
use cirka_diagnostics::{Diagnostic, Span};

type R<T> = Result<T, ()>;

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    pub errs: Vec<Diagnostic>,
}

/// Parses a module from layout-processed tokens.
pub fn parse_module(toks: Vec<Token>) -> (Module, Vec<Diagnostic>) {
    let mut p = Parser {
        toks,
        pos: 0,
        errs: Vec::new(),
    };
    let m = p.module();
    (m, p.errs)
}

/// Parses a single expression (tests and the REPL-like helpers).
pub fn parse_expr_tokens(toks: Vec<Token>) -> (Option<Expr>, Vec<Diagnostic>) {
    let mut p = Parser {
        toks,
        pos: 0,
        errs: Vec::new(),
    };
    while p.at(&Tok::VSemi) {
        p.bump();
    }
    let e = p.expr().ok();
    (e, p.errs)
}

/// The constraints written before `=>`.
fn context_items(e: Expr) -> Vec<Expr> {
    match e.kind {
        ExprKind::Tuple(items) => items,
        ExprKind::Unit => Vec::new(),
        _ => vec![e],
    }
}

impl Parser {
    // ---- token helpers ----

    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].tok
    }

    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos.min(self.toks.len() - 1)].span
    }

    fn prev_end(&self) -> Span {
        if self.pos == 0 {
            return self.span();
        }
        // Skip virtual tokens backwards (they have empty spans).
        let mut i = self.pos - 1;
        while i > 0 && matches!(self.toks[i].tok, Tok::VLBrace | Tok::VSemi | Tok::VRBrace) {
            i -= 1;
        }
        self.toks[i].span
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn at_kw(&self, k: Kw) -> bool {
        *self.peek() == Tok::Kw(k)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error_here(&mut self, what: &str) {
        let found = self.peek().describe();
        let sp = self.span();
        self.errs.push(
            Diagnostic::error(format!("expected {what}, found {found}"))
                .with_code("E1002")
                .with_label(sp, format!("expected {what}")),
        );
    }

    fn expect(&mut self, t: &Tok, what: &str) -> R<Span> {
        if self.at(t) {
            Ok(self.bump().span)
        } else {
            self.error_here(what);
            Err(())
        }
    }

    fn from(&self, start: Span) -> Span {
        start.to(self.prev_end())
    }

    // ---- blocks ----

    fn block_open(&mut self) -> R<bool> {
        match self.peek() {
            Tok::VLBrace => {
                self.bump();
                Ok(true)
            }
            Tok::LBrace => {
                self.bump();
                Ok(false)
            }
            _ => {
                self.error_here("a block");
                Err(())
            }
        }
    }

    /// Parses `{ item ; item ; ... }` (virtual or explicit), recovering
    /// from errors at item boundaries.
    fn block<T>(&mut self, mut item: impl FnMut(&mut Self) -> R<T>) -> R<Vec<T>> {
        let virt = self.block_open()?;
        let (sep, close) = if virt {
            (Tok::VSemi, Tok::VRBrace)
        } else {
            (Tok::Semi, Tok::RBrace)
        };
        let mut out = Vec::new();
        loop {
            while self.eat(&sep) {}
            if self.eat(&close) {
                return Ok(out);
            }
            if self.at(&Tok::Eof) {
                if !virt {
                    self.error_here("`}`");
                }
                return Ok(out);
            }
            let start = self.pos;
            match item(self) {
                Ok(x) => {
                    out.push(x);
                    if !self.at(&sep) && !self.at(&close) {
                        if virt && self.at_layout_closer() {
                            // e.g. `)` closing an implicit block on the same line:
                            // the layout pass already inserted the brace.
                        }
                        self.error_here(if virt { "end of line" } else { "`;` or `}`" });
                        self.recover(&sep, &close);
                    }
                }
                Err(()) => {
                    self.recover(&sep, &close);
                    if self.pos == start {
                        self.bump();
                    }
                }
            }
        }
    }

    fn at_layout_closer(&self) -> bool {
        matches!(self.peek(), Tok::RParen | Tok::RBracket | Tok::Comma)
    }

    /// Skips to the next separator or closer of the current block.
    fn recover(&mut self, sep: &Tok, close: &Tok) {
        let mut depth = 0i32;
        loop {
            let t = self.peek().clone();
            match t {
                Tok::Eof => return,
                Tok::VLBrace | Tok::LBrace => depth += 1,
                Tok::VRBrace | Tok::RBrace => {
                    if depth == 0 {
                        if &t == close {
                            return;
                        }
                    } else {
                        depth -= 1;
                    }
                }
                _ if depth == 0 && &t == sep => return,
                _ => {}
            }
            self.bump();
        }
    }

    // ---- module ----

    fn module(&mut self) -> Module {
        let mut m = Module {
            name: None,
            imports: Vec::new(),
            decls: Vec::new(),
        };
        // The top level is an implicit block without an opening brace.
        loop {
            while self.eat(&Tok::VSemi) {}
            if self.at(&Tok::VRBrace) || self.at(&Tok::Eof) {
                break;
            }
            let start = self.pos;
            let r = self.top_item(&mut m);
            let ok =
                r.is_ok() && (self.at(&Tok::VSemi) || self.at(&Tok::VRBrace) || self.at(&Tok::Eof));
            if r.is_ok() && !ok {
                self.error_here("end of the declaration");
            }
            if !ok {
                self.recover(&Tok::VSemi, &Tok::VRBrace);
                if self.pos == start {
                    self.bump();
                }
            }
        }
        while !self.at(&Tok::Eof) {
            if !matches!(self.peek(), Tok::VRBrace | Tok::VSemi) {
                self.error_here("a declaration at column 1");
                break;
            }
            self.bump();
        }
        m
    }

    fn top_item(&mut self, m: &mut Module) -> R<()> {
        let start = self.span();
        match self.peek() {
            Tok::Kw(Kw::Module) => {
                self.bump();
                let path = self.modname()?;
                if m.name.is_some() || !m.decls.is_empty() || !m.imports.is_empty() {
                    self.errs.push(
                        Diagnostic::error("`module` must be the first declaration")
                            .with_code("E1003")
                            .with_label(self.from(start), "move this to the top of the file"),
                    );
                }
                m.name = Some((path, self.from(start)));
                Ok(())
            }
            Tok::Kw(Kw::Import) => {
                self.bump();
                let path = self.modname()?;
                // `as` is a keyword only here.
                let alias = if matches!(self.peek(), Tok::LIdent(q, n) if q.is_empty() && n == "as")
                {
                    self.bump();
                    let s = self.span();
                    let a = self.modname()?;
                    Some((a, self.from(s)))
                } else {
                    None
                };
                let names = if self.at(&Tok::LParen) {
                    Some(self.import_names()?)
                } else {
                    None
                };
                if !m.decls.is_empty() {
                    self.errs.push(
                        Diagnostic::error("`import` must come before the declarations")
                            .with_code("E1003")
                            .with_label(self.from(start), "move this import up"),
                    );
                }
                m.imports.push(Import {
                    path,
                    alias,
                    names,
                    span: self.from(start),
                });
                Ok(())
            }
            _ => {
                let d = self.decl()?;
                m.decls.push(d);
                Ok(())
            }
        }
    }

    /// `(x, T, (+))`: the names of an import list.
    fn import_names(&mut self) -> R<Vec<(String, Span)>> {
        self.expect(&Tok::LParen, "`(`")?;
        let mut names = Vec::new();
        if self.at(&Tok::RParen) {
            self.bump();
            return Ok(names);
        }
        loop {
            let s = self.span();
            match self.peek().clone() {
                Tok::LIdent(q, n) | Tok::UIdent(q, n) if q.is_empty() => {
                    self.bump();
                    names.push((n, self.from(s)));
                }
                Tok::LParen => {
                    self.bump();
                    match self.peek().clone() {
                        Tok::Op(q, o) if q.is_empty() => {
                            self.bump();
                            self.expect(&Tok::RParen, "`)`")?;
                            names.push((o, self.from(s)));
                        }
                        _ => {
                            self.error_here("an operator");
                            return Err(());
                        }
                    }
                }
                _ => {
                    self.error_here("a name to import");
                    return Err(());
                }
            }
            if self.at(&Tok::Comma) {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(&Tok::RParen, "`)` or `,`")?;
        Ok(names)
    }

    fn modname(&mut self) -> R<Vec<String>> {
        match self.peek().clone() {
            Tok::UIdent(mut path, last) => {
                self.bump();
                path.push(last);
                Ok(path)
            }
            _ => {
                self.error_here("a module name");
                Err(())
            }
        }
    }

    fn decl(&mut self) -> R<Decl> {
        let start = self.span();
        let private = self.eat(&Tok::Kw(Kw::Private));
        let kind = match self.peek().clone() {
            Tok::Kw(Kw::Data) => DeclKind::Data(self.data()?),
            Tok::Kw(k @ (Kw::Infix | Kw::Infixl | Kw::Infixr)) => {
                self.bump();
                let assoc = match k {
                    Kw::Infixl => Assoc::Left,
                    Kw::Infixr => Assoc::Right,
                    _ => Assoc::None,
                };
                let prec = match self.peek().clone() {
                    Tok::Int(n) if n <= 9 => {
                        self.bump();
                        n as u8
                    }
                    _ => {
                        self.error_here("a precedence between 0 and 9");
                        return Err(());
                    }
                };
                let mut ops = Vec::new();
                loop {
                    let sp = self.span();
                    match self.peek().clone() {
                        Tok::Op(path, name) if path.is_empty() => {
                            self.bump();
                            ops.push((name, sp));
                        }
                        Tok::Backtick => {
                            self.bump();
                            let name = match self.peek().clone() {
                                Tok::LIdent(p, n) | Tok::UIdent(p, n) if p.is_empty() => {
                                    self.bump();
                                    n
                                }
                                _ => {
                                    self.error_here("a name");
                                    return Err(());
                                }
                            };
                            self.expect(&Tok::Backtick, "`` ` ``")?;
                            ops.push((name, self.from(sp)));
                        }
                        _ => {
                            self.error_here("an operator");
                            return Err(());
                        }
                    }
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                DeclKind::Fixity { assoc, prec, ops }
            }
            Tok::Kw(Kw::Prim) => {
                self.bump();
                // Primitive types have upper-case names (`prim Int : Type`).
                let (name, name_span) = match self.peek().clone() {
                    Tok::UIdent(p, n) if p.is_empty() => (n, self.bump().span),
                    _ => self.def_name()?,
                };
                self.expect(&Tok::Colon, "`:`")?;
                let ty = self.expr()?;
                DeclKind::Prim {
                    name,
                    name_span,
                    ty,
                }
            }
            Tok::Kw(Kw::Trait) => DeclKind::Trait(self.trait_decl()?),
            Tok::Kw(Kw::Impl) => {
                if private {
                    let sp = self.span();
                    self.errs.push(
                        Diagnostic::error("`impl` cannot be private")
                            .with_code("E1006")
                            .with_label(sp, "instances are visible wherever their trait is"),
                    );
                }
                DeclKind::Impl(self.impl_decl()?)
            }
            Tok::Kw(k @ (Kw::Total | Kw::Partial | Kw::Covering | Kw::Mutual)) => {
                let sp = self.span();
                self.errs.push(
                    Diagnostic::error(format!(
                        "`{}` is not supported in CirkaLang v0.0.2",
                        k.name()
                    ))
                    .with_code("E1004")
                    .with_label(sp, "reserved for a later version"),
                );
                return Err(());
            }
            Tok::LIdent(path, w)
                if path.is_empty() && w == "test" && matches!(self.peek_at(1), Tok::Str(_)) =>
            {
                self.bump();
                let name_span = self.span();
                let Tok::Str(name) = self.bump().tok else {
                    unreachable!()
                };
                self.expect(&Tok::Equals, "`=`")?;
                let body = self.expr()?;
                DeclKind::Test {
                    name,
                    name_span,
                    body,
                }
            }
            Tok::LIdent(path, w)
                if path.is_empty() && w == "extern" && matches!(self.peek_at(1), Tok::Str(_)) =>
            {
                DeclKind::Extern(self.extern_block()?)
            }
            Tok::LIdent(path, w)
                if path.is_empty()
                    && w == "tests"
                    && matches!(self.peek_at(1), Tok::Kw(Kw::Where)) =>
            {
                self.bump();
                self.bump();
                let ds = self.block(|p| p.decl())?;
                for d in &ds {
                    if let DeclKind::Tests(_) = d.kind {
                        self.errs.push(
                            Diagnostic::error("`tests` blocks cannot be nested")
                                .with_code("E1003")
                                .with_label(d.span, "move this block to the top level"),
                        );
                    }
                }
                DeclKind::Tests(ds)
            }
            _ => {
                if self.is_sig_start() {
                    let mut names = vec![self.def_name()?];
                    while self.eat(&Tok::Comma) {
                        names.push(self.def_name()?);
                    }
                    self.expect(&Tok::Colon, "`:`")?;
                    let ty = self.expr()?;
                    DeclKind::Sig { names, ty }
                } else {
                    self.clause()?
                }
            }
        };
        Ok(Decl {
            private,
            kind,
            span: self.from(start),
        })
    }

    /// `extern "C" [link "lib"] where { name : type; .. }` (after `extern`).
    fn extern_block(&mut self) -> R<ExternBlock> {
        self.bump();
        let abi_span = self.span();
        let Tok::Str(abi) = self.bump().tok else {
            unreachable!()
        };
        let abi = match abi.as_str() {
            "C" => Abi::C,
            "cirka" => Abi::Cirka,
            _ => {
                self.errs.push(
                    Diagnostic::error(format!("unknown calling convention `{abi}`"))
                        .with_code("E1007")
                        .with_label(abi_span, "expected \"C\" or \"cirka\""),
                );
                Abi::C
            }
        };
        let link = match self.peek().clone() {
            Tok::LIdent(p, w) if p.is_empty() && w == "link" => {
                self.bump();
                let sp = self.span();
                match self.peek().clone() {
                    Tok::Str(l) => {
                        self.bump();
                        // Programs are linked with these; other libraries
                        // need the linker options of FEAT-0047.
                        if !matches!(l.as_str(), "c" | "m" | "pthread") {
                            self.errs.push(
                                Diagnostic::error(format!("cannot link library `{l}`"))
                                    .with_code("E1007")
                                    .with_label(sp, "only \"c\", \"m\" and \"pthread\" in v0.0.2"),
                            );
                        }
                        Some((l, sp))
                    }
                    _ => {
                        self.error_here("a library name");
                        return Err(());
                    }
                }
            }
            _ => None,
        };
        self.expect(&Tok::Kw(Kw::Where), "`where`")?;
        let sigs = self.block(|p| {
            let (name, name_span) = match p.peek().clone() {
                Tok::LIdent(path, n) if path.is_empty() => (n, p.bump().span),
                _ => {
                    p.error_here("the name of a function");
                    return Err(());
                }
            };
            p.expect(&Tok::Colon, "`:`")?;
            let ty = p.expr()?;
            Ok(ExternSig {
                name,
                name_span,
                ty,
            })
        })?;
        Ok(ExternBlock {
            abi,
            abi_span,
            link,
            sigs,
        })
    }

    /// `name :` or `name ,` or `(op) :`.
    fn is_sig_start(&self) -> bool {
        match (self.peek(), self.peek_at(1)) {
            (Tok::LIdent(p, _), Tok::Colon | Tok::Comma) => p.is_empty(),
            (Tok::LParen, Tok::Op(..)) => {
                matches!(self.peek_at(2), Tok::RParen)
                    && matches!(self.peek_at(3), Tok::Colon | Tok::Comma)
            }
            _ => false,
        }
    }

    /// A defined name: `x` or `(op)`.
    fn def_name(&mut self) -> R<(String, Span)> {
        let sp = self.span();
        match self.peek().clone() {
            Tok::LIdent(p, n) if p.is_empty() => {
                self.bump();
                Ok((n, sp))
            }
            Tok::LParen => {
                self.bump();
                let n = match self.peek().clone() {
                    Tok::Op(p, n) if p.is_empty() => {
                        self.bump();
                        n
                    }
                    _ => {
                        self.error_here("an operator");
                        return Err(());
                    }
                };
                self.expect(&Tok::RParen, "`)`")?;
                Ok((n, self.from(sp)))
            }
            _ => {
                self.error_here("a name");
                Err(())
            }
        }
    }

    fn clause(&mut self) -> R<DeclKind> {
        // lhs ::= item { item } with items being apats or operators.
        let mut pats: Vec<Pat> = Vec::new();
        let mut ops: Vec<(usize, OpRef)> = Vec::new();
        let start = self.span();
        loop {
            match self.peek().clone() {
                Tok::Equals => break,
                Tok::Op(path, name)
                    if path.is_empty()
                        && !(name == "-" && matches!(self.peek_at(1), Tok::Int(_))) =>
                {
                    let sp = self.bump().span;
                    ops.push((
                        pats.len(),
                        OpRef {
                            name: QName::simple(name),
                            span: sp,
                            backtick: false,
                        },
                    ));
                }
                Tok::Backtick => {
                    let o = self.backtick_op()?;
                    ops.push((pats.len(), o));
                }
                _ => {
                    if pats.is_empty()
                        && ops.is_empty()
                        && matches!(self.peek(), Tok::LParen)
                        && matches!(self.peek_at(1), Tok::Op(..))
                        && matches!(self.peek_at(2), Tok::RParen)
                    {
                        // `(op) x y = ...`
                        let (n, sp) = self.def_name()?;
                        pats.push(Pat {
                            kind: PatKind::Var(n),
                            span: sp,
                        });
                        continue;
                    }
                    if !self.starts_apat() {
                        self.error_here("a pattern or `=`");
                        return Err(());
                    }
                    pats.push(self.apat()?);
                }
            }
        }
        let lhs_span = self.from(start);
        self.expect(&Tok::Equals, "`=`")?;
        let rhs = self.expr()?;
        let var_ops: Vec<&(usize, OpRef)> = ops.iter().filter(|(_, o)| !o.name.is_con()).collect();
        match var_ops.as_slice() {
            [] => {
                if !ops.is_empty() {
                    self.errs.push(
                        Diagnostic::error("a definition must start with the name being defined")
                            .with_code("E1005")
                            .with_label(lhs_span, "put constructor patterns in parentheses"),
                    );
                    return Err(());
                }
                let mut it = pats.into_iter();
                match it.next() {
                    Some(Pat {
                        kind: PatKind::Var(name),
                        span,
                    }) => Ok(DeclKind::Clause {
                        name,
                        name_span: span,
                        pats: it.collect(),
                        rhs,
                    }),
                    _ => {
                        self.errs.push(
                            Diagnostic::error(
                                "a definition must start with the name being defined",
                            )
                            .with_code("E1005")
                            .with_label(lhs_span, "expected `name patterns = expression`"),
                        );
                        Err(())
                    }
                }
            }
            [(i, op)] => {
                let i = *i;
                let op = op.clone();
                let mk = |pats: Vec<Pat>, ops: Vec<OpRef>| -> Option<Pat> {
                    match (pats.len(), ops.len()) {
                        (1, 0) => pats.into_iter().next(),
                        (n, m) if n == m + 1 && n > 1 => {
                            let sp = pats[0].span.to(pats[n - 1].span);
                            Some(Pat {
                                kind: PatKind::OpSeq(pats, ops),
                                span: sp,
                            })
                        }
                        _ => None,
                    }
                };
                let (lp, rp): (Vec<Pat>, Vec<Pat>) = {
                    let mut l = pats;
                    let r = l.split_off(i);
                    (l, r)
                };
                let lo: Vec<OpRef> = ops
                    .iter()
                    .filter(|(j, o)| *j < i && o.name.is_con())
                    .map(|x| x.1.clone())
                    .collect();
                let ro: Vec<OpRef> = ops
                    .iter()
                    .filter(|(j, o)| *j > i && o.name.is_con())
                    .map(|x| x.1.clone())
                    .collect();
                match (mk(lp, lo), mk(rp, ro)) {
                    (Some(l), Some(r)) => Ok(DeclKind::Clause {
                        name: op.name.name,
                        name_span: op.span,
                        pats: vec![l, r],
                        rhs,
                    }),
                    _ => {
                        self.errs.push(
                            Diagnostic::error("malformed operator definition")
                                .with_code("E1005")
                                .with_label(lhs_span, "expected `pattern op pattern = expression`"),
                        );
                        Err(())
                    }
                }
            }
            _ => {
                self.errs.push(
                    Diagnostic::error("more than one operator on the left-hand side")
                        .with_code("E1005")
                        .with_label(lhs_span, "put patterns with operators in parentheses"),
                );
                Err(())
            }
        }
    }

    /// `ctx =>` if the following tokens start with one.
    fn opt_context(&mut self) -> R<Vec<Expr>> {
        let save = (self.pos, self.errs.len());
        if let Ok(e) = self.opseq()
            && self.at(&Tok::FatArrow)
        {
            self.bump();
            return Ok(context_items(e));
        }
        self.pos = save.0;
        self.errs.truncate(save.1);
        Ok(Vec::new())
    }

    fn type_name(&mut self) -> R<(String, Span)> {
        let sp = self.span();
        match self.peek().clone() {
            Tok::UIdent(p, n) if p.is_empty() => {
                self.bump();
                Ok((n, sp))
            }
            _ => {
                self.error_here("a type name");
                Err(())
            }
        }
    }

    fn trait_decl(&mut self) -> R<TraitDecl> {
        self.bump(); // trait
        let context = self.opt_context()?;
        let (name, name_span) = self.type_name()?;
        let params = self.type_params()?;
        let mut items = Vec::new();
        if self.at_kw(Kw::Where) {
            self.bump();
            items = self.block(|p| p.decl())?;
        }
        for d in &items {
            if !matches!(d.kind, DeclKind::Sig { .. } | DeclKind::Clause { .. }) || d.private {
                self.errs.push(
                    Diagnostic::error(
                        "a trait may contain only method signatures and default definitions",
                    )
                    .with_code("E1006")
                    .with_label(d.span, "not allowed in a `trait` block"),
                );
            }
        }
        Ok(TraitDecl {
            context,
            name,
            name_span,
            params,
            items,
        })
    }

    fn impl_decl(&mut self) -> R<ImplDecl> {
        self.bump(); // impl
        // `{a, b : K} ->` declares the sort of variables of the head.
        let mut binders = Vec::new();
        while self.at(&Tok::LBrace) {
            self.bump();
            let names = self.name_list()?;
            self.expect(&Tok::Colon, "`:`")?;
            let k = self.expr()?;
            self.expect(&Tok::RBrace, "`}`")?;
            self.expect(&Tok::Arrow, "`->`")?;
            for (n, sp) in names {
                binders.push((n, sp, k.clone()));
            }
        }
        let context = self.opt_context()?;
        let head = self.opseq()?;
        let mut items = Vec::new();
        if self.at_kw(Kw::Where) {
            self.bump();
            items = self.block(|p| p.decl())?;
        }
        for d in &items {
            if !matches!(d.kind, DeclKind::Clause { .. }) || d.private {
                self.errs.push(
                    Diagnostic::error("an `impl` may contain only method definitions")
                        .with_code("E1006")
                        .with_label(d.span, "not allowed in an `impl` block"),
                );
            }
        }
        Ok(ImplDecl {
            binders,
            context,
            head,
            items,
        })
    }

    /// `deriving C`, `deriving (C, D)`.
    fn deriving(&mut self) -> R<Vec<(QName, Span)>> {
        self.bump(); // deriving
        let mut out = Vec::new();
        let one = |p: &mut Self, out: &mut Vec<(QName, Span)>| -> R<()> {
            let sp = p.span();
            match p.peek().clone() {
                Tok::UIdent(m, n) => {
                    p.bump();
                    out.push((QName { module: m, name: n }, sp));
                    Ok(())
                }
                _ => {
                    p.error_here("a trait name");
                    Err(())
                }
            }
        };
        if self.eat(&Tok::LParen) {
            if !self.eat(&Tok::RParen) {
                loop {
                    one(self, &mut out)?;
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, "`)` or `,`")?;
            }
        } else {
            one(self, &mut out)?;
        }
        Ok(out)
    }

    fn type_params(&mut self) -> R<Vec<Param>> {
        let mut params = Vec::new();
        loop {
            match self.peek().clone() {
                Tok::LIdent(p, n) if p.is_empty() => {
                    let sp = self.bump().span;
                    params.push(Param {
                        mult: None,
                        names: vec![(n, sp)],
                        ty: None,
                    });
                }
                Tok::LParen => {
                    self.bump();
                    let mult = self.opt_mult();
                    let names = self.name_list()?;
                    self.expect(&Tok::Colon, "`:`")?;
                    let ty = self.expr()?;
                    self.expect(&Tok::RParen, "`)`")?;
                    params.push(Param {
                        mult,
                        names,
                        ty: Some(ty),
                    });
                }
                _ => break,
            }
        }
        Ok(params)
    }

    fn data(&mut self) -> R<DataDecl> {
        self.bump(); // data
        let (name, name_span) = self.type_name()?;
        let params = self.type_params()?;
        if self.eat(&Tok::Equals) {
            let mut cons = vec![self.short_con()?];
            while self.eat(&Tok::Bar) {
                cons.push(self.short_con()?);
            }
            let deriving = if self.at_kw(Kw::Deriving) {
                self.deriving()?
            } else {
                Vec::new()
            };
            return Ok(DataDecl {
                name,
                name_span,
                params,
                sig: None,
                cons,
                short: true,
                deriving,
            });
        }
        let sig = if self.eat(&Tok::Colon) {
            Some(self.expr()?)
        } else {
            None
        };
        if !self.at_kw(Kw::Where) {
            self.error_here("`where` or `=`");
            return Err(());
        }
        self.bump();
        let mut deriving = Vec::new();
        let groups = self.block(|p| {
            if p.at_kw(Kw::Deriving) {
                deriving.extend(p.deriving()?);
                return Ok(Vec::new());
            }
            let mut names = vec![p.con_name()?];
            while p.eat(&Tok::Comma) {
                names.push(p.con_name()?);
            }
            p.expect(&Tok::Colon, "`:`")?;
            let ty = p.expr()?;
            Ok(names
                .into_iter()
                .map(|(n, sp)| ConDecl {
                    name: n,
                    span: sp,
                    body: ConBody::Sig(ty.clone()),
                })
                .collect::<Vec<_>>())
        })?;
        if self.at_kw(Kw::Deriving) {
            deriving.extend(self.deriving()?);
        }
        Ok(DataDecl {
            name,
            name_span,
            params,
            sig,
            cons: groups.into_iter().flatten().collect(),
            short: false,
            deriving,
        })
    }

    fn con_name(&mut self) -> R<(String, Span)> {
        let sp = self.span();
        match self.peek().clone() {
            Tok::UIdent(p, n) if p.is_empty() => {
                self.bump();
                Ok((n, sp))
            }
            Tok::LParen => {
                self.bump();
                let n = match self.peek().clone() {
                    Tok::Op(p, n) if p.is_empty() && n.starts_with(':') => {
                        self.bump();
                        n
                    }
                    _ => {
                        self.error_here("a constructor operator (starting with `:`)");
                        return Err(());
                    }
                };
                self.expect(&Tok::RParen, "`)`")?;
                Ok((n, self.from(sp)))
            }
            _ => {
                self.error_here("a constructor name");
                Err(())
            }
        }
    }

    fn short_con(&mut self) -> R<ConDecl> {
        let start = self.span();
        if matches!(self.peek(), Tok::UIdent(p, _) if p.is_empty())
            || (self.at(&Tok::LParen)
                && matches!(self.peek_at(1), Tok::Op(_, n) if n.starts_with(':')))
        {
            let (name, span) = self.con_name()?;
            let mut fields = Vec::new();
            while self.starts_atom() {
                fields.push(self.atom()?);
            }
            // `a :: List a` form starting with an upper-case type.
            if let Tok::Op(p, op) = self.peek().clone()
                && p.is_empty()
                && op.starts_with(':')
            {
                let lhs_span = self.from(start);
                let lhs = if fields.is_empty() {
                    Expr {
                        kind: ExprKind::Var(QName::simple(name)),
                        span,
                    }
                } else {
                    fields.into_iter().fold(
                        Expr {
                            kind: ExprKind::Var(QName::simple(name)),
                            span,
                        },
                        |f, a| Expr {
                            span: lhs_span,
                            kind: ExprKind::App(Box::new(f), Box::new(a)),
                        },
                    )
                };
                let sp = self.bump().span;
                let rhs = self.app()?;
                return Ok(ConDecl {
                    name: op,
                    span: sp,
                    body: ConBody::Fields(vec![lhs, rhs]),
                });
            }
            return Ok(ConDecl {
                name,
                span,
                body: ConBody::Fields(fields),
            });
        }
        let lhs = self.app()?;
        match self.peek().clone() {
            Tok::Op(p, op) if p.is_empty() && op.starts_with(':') => {
                let sp = self.bump().span;
                let rhs = self.app()?;
                Ok(ConDecl {
                    name: op,
                    span: sp,
                    body: ConBody::Fields(vec![lhs, rhs]),
                })
            }
            _ => {
                self.error_here("a constructor");
                Err(())
            }
        }
    }

    fn opt_mult(&mut self) -> Option<Mult> {
        match self.peek() {
            Tok::Int(0) if matches!(self.peek_at(1), Tok::LIdent(..) | Tok::Underscore) => {
                self.bump();
                Some(Mult::Zero)
            }
            Tok::Int(1) if matches!(self.peek_at(1), Tok::LIdent(..) | Tok::Underscore) => {
                self.bump();
                Some(Mult::One)
            }
            Tok::Omega if matches!(self.peek_at(1), Tok::LIdent(..) | Tok::Underscore) => {
                self.bump();
                Some(Mult::Many)
            }
            _ => None,
        }
    }

    fn name_list(&mut self) -> R<Vec<(String, Span)>> {
        let mut out = Vec::new();
        loop {
            let sp = self.span();
            match self.peek().clone() {
                Tok::LIdent(p, n) if p.is_empty() => {
                    self.bump();
                    out.push((n, sp));
                }
                Tok::Underscore => {
                    self.bump();
                    out.push(("_".into(), sp));
                }
                _ => {
                    self.error_here("a variable name");
                    return Err(());
                }
            }
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    // ---- expressions ----

    pub(crate) fn expr(&mut self) -> R<Expr> {
        let start = self.span();
        if let Some(e) = self.borrow_sugar()? {
            return Ok(e);
        }
        match self.peek().clone() {
            Tok::Backslash => {
                self.bump();
                let mut bs = vec![self.lam_binder()?];
                while self.eat(&Tok::Comma) {
                    bs.push(self.lam_binder()?);
                }
                self.expect(&Tok::FatArrow, "`=>`")?;
                let body = self.expr()?;
                Ok(Expr {
                    kind: ExprKind::Lam(bs, Box::new(body)),
                    span: self.from(start),
                })
            }
            Tok::Kw(Kw::Let) => {
                self.bump();
                let bs = self.block(|p| p.let_bind())?;
                if !self.at_kw(Kw::In) {
                    self.error_here("`in`");
                    return Err(());
                }
                self.bump();
                let body = self.expr()?;
                Ok(Expr {
                    kind: ExprKind::Let(bs, Box::new(body)),
                    span: self.from(start),
                })
            }
            Tok::Kw(Kw::Case) => {
                self.bump();
                let scrut = self.expr()?;
                if !self.at_kw(Kw::Of) {
                    self.error_here("`of`");
                    return Err(());
                }
                self.bump();
                let alts = self.block(|p| {
                    let pat = p.pat()?;
                    p.expect(&Tok::FatArrow, "`=>`")?;
                    let rhs = p.expr()?;
                    Ok(Alt { pat, rhs })
                })?;
                Ok(Expr {
                    kind: ExprKind::Case(Box::new(scrut), alts),
                    span: self.from(start),
                })
            }
            Tok::Kw(Kw::If) => {
                self.bump();
                let c = self.expr()?;
                if !self.at_kw(Kw::Then) {
                    self.error_here("`then`");
                    return Err(());
                }
                self.bump();
                let t = self.expr()?;
                // Allow `else` at the start of a line in a `do` block.
                if self.at(&Tok::VSemi) && *self.peek_at(1) == Tok::Kw(Kw::Else) {
                    self.bump();
                }
                if !self.at_kw(Kw::Else) {
                    self.error_here("`else`");
                    return Err(());
                }
                self.bump();
                let e = self.expr()?;
                Ok(Expr {
                    kind: ExprKind::If(Box::new(c), Box::new(t), Box::new(e)),
                    span: self.from(start),
                })
            }
            Tok::Kw(Kw::Do) => {
                self.bump();
                let stmts = self.block(|p| p.stmt())?;
                Ok(Expr {
                    kind: ExprKind::Do(stmts),
                    span: self.from(start),
                })
            }
            Tok::LBrace => {
                // Implicit binder `{π x, y : A} -> B`.
                self.bump();
                let mult = self.opt_mult();
                let names = self.name_list()?;
                let dom = if self.eat(&Tok::Colon) {
                    self.expr()?
                } else {
                    Expr {
                        kind: ExprKind::Wildcard,
                        span: self.prev_end(),
                    }
                };
                self.expect(&Tok::RBrace, "`}`")?;
                self.expect(&Tok::Arrow, "`->`")?;
                let cod = self.expr()?;
                Ok(Expr {
                    kind: ExprKind::Pi {
                        mult,
                        implicit: true,
                        names,
                        dom: Box::new(dom),
                        cod: Box::new(cod),
                    },
                    span: self.from(start),
                })
            }
            Tok::LParen if self.looks_like_binder() => {
                let save = (self.pos, self.errs.len());
                if let Ok((mult, names, dom)) = self.explicit_binder()
                    && self.at(&Tok::Arrow)
                {
                    self.bump();
                    let cod = self.expr()?;
                    return Ok(Expr {
                        kind: ExprKind::Pi {
                            mult,
                            implicit: false,
                            names,
                            dom: Box::new(dom),
                            cod: Box::new(cod),
                        },
                        span: self.from(start),
                    });
                }
                self.pos = save.0;
                self.errs.truncate(save.1);
                self.arrow_expr(start)
            }
            _ => self.arrow_expr(start),
        }
    }

    /// `(` [0|1|ω] name {, name} `:` ...
    fn looks_like_binder(&self) -> bool {
        let mut k = 1;
        if matches!(self.peek_at(k), Tok::Int(0 | 1) | Tok::Omega) {
            k += 1;
        }
        loop {
            if !matches!(self.peek_at(k), Tok::LIdent(p, _) if p.is_empty())
                && !matches!(self.peek_at(k), Tok::Underscore)
            {
                return false;
            }
            k += 1;
            match self.peek_at(k) {
                Tok::Comma => k += 1,
                Tok::Colon => return true,
                _ => return false,
            }
        }
    }

    fn explicit_binder(&mut self) -> R<(Option<Mult>, Vec<(String, Span)>, Expr)> {
        self.expect(&Tok::LParen, "`(`")?;
        let mult = self.opt_mult();
        let names = self.name_list()?;
        self.expect(&Tok::Colon, "`:`")?;
        let dom = self.expr()?;
        self.expect(&Tok::RParen, "`)`")?;
        Ok((mult, names, dom))
    }

    fn arrow_expr(&mut self, start: Span) -> R<Expr> {
        let lhs = self.opseq()?;
        if self.eat(&Tok::Arrow) {
            let cod = self.expr()?;
            return Ok(Expr {
                kind: ExprKind::Pi {
                    mult: None,
                    implicit: false,
                    names: Vec::new(),
                    dom: Box::new(lhs),
                    cod: Box::new(cod),
                },
                span: self.from(start),
            });
        }
        // `C a => T`, `(C a, D b) => T`: instance-implicit binders.
        if self.eat(&Tok::FatArrow) {
            let cod = self.expr()?;
            let span = self.from(start);
            return Ok(context_items(lhs)
                .into_iter()
                .rev()
                .fold(cod, |cod, c| Expr {
                    span,
                    kind: ExprKind::Pi {
                        mult: Some(Mult::Many),
                        implicit: true,
                        names: Vec::new(),
                        dom: Box::new(c),
                        cod: Box::new(cod),
                    },
                }));
        }
        Ok(lhs)
    }

    fn backtick_op(&mut self) -> R<OpRef> {
        let sp = self.span();
        self.expect(&Tok::Backtick, "`` ` ``")?;
        let name = match self.peek().clone() {
            Tok::LIdent(m, n) | Tok::UIdent(m, n) => {
                self.bump();
                QName { module: m, name: n }
            }
            _ => {
                self.error_here("a name");
                return Err(());
            }
        };
        self.expect(&Tok::Backtick, "`` ` ``")?;
        Ok(OpRef {
            name,
            span: self.from(sp),
            backtick: true,
        })
    }

    /// An operator at the current position, unless it is followed by `)`
    /// (a left section, handled by the caller).
    fn at_binop(&self) -> bool {
        match self.peek() {
            Tok::Op(..) => !matches!(self.peek_at(1), Tok::RParen),
            Tok::Backtick => !matches!(self.peek_at(3), Tok::RParen),
            _ => false,
        }
    }

    fn opseq(&mut self) -> R<Expr> {
        let start = self.span();
        let neg = match self.peek() {
            Tok::Op(p, n) if p.is_empty() && n == "-" => Some(self.bump().span),
            _ => None,
        };
        let first = self.operand()?;
        let mut operands = vec![first];
        let mut ops = Vec::new();
        while self.at_binop() {
            let o = if self.at(&Tok::Backtick) {
                self.backtick_op()?
            } else {
                let t = self.bump();
                let Tok::Op(m, n) = t.tok else { unreachable!() };
                OpRef {
                    name: QName { module: m, name: n },
                    span: t.span,
                    backtick: false,
                }
            };
            ops.push(o);
            let last = matches!(
                self.peek(),
                Tok::Backslash | Tok::Kw(Kw::Let | Kw::If | Kw::Case | Kw::Do)
            );
            operands.push(self.operand()?);
            if last {
                break;
            }
        }
        if ops.is_empty() && neg.is_none() {
            return Ok(operands.pop().unwrap());
        }
        Ok(Expr {
            kind: ExprKind::OpSeq { neg, operands, ops },
            span: self.from(start),
        })
    }

    /// `borrow x as a in e` is `borrow x (\_, a => e)`, and likewise for
    /// `borrowIO` (spec-v0.0.3.md §2.2): the region is not named.
    fn borrow_sugar(&mut self) -> R<Option<Expr>> {
        let start = self.span();
        let f = match self.peek() {
            Tok::LIdent(q, n) if q.is_empty() && (n == "borrow" || n == "borrowIO") => n.clone(),
            _ => return Ok(None),
        };
        let save = (self.pos, self.errs.len());
        self.bump();
        let x = match self.atom() {
            Ok(x) if matches!(self.peek(), Tok::LIdent(q, n) if q.is_empty() && n == "as") => x,
            _ => {
                self.pos = save.0;
                self.errs.truncate(save.1);
                return Ok(None);
            }
        };
        self.bump(); // as
        let sp = self.span();
        let name = match self.peek().clone() {
            Tok::LIdent(q, n) if q.is_empty() => {
                self.bump();
                n
            }
            _ => {
                self.error_here("a variable name");
                return Err(());
            }
        };
        if !self.at_kw(Kw::In) {
            self.error_here("`in`");
            return Err(());
        }
        self.bump();
        let body = self.expr()?;
        let span = self.from(start);
        let bind = |name: Option<String>, span| LamBinder {
            name,
            span,
            mult: None,
            ty: None,
        };
        let lam = Expr {
            kind: ExprKind::Lam(
                vec![bind(None, start), bind(Some(name), sp)],
                Box::new(body),
            ),
            span,
        };
        let head = Expr {
            kind: ExprKind::Var(QName::simple(f)),
            span: start,
        };
        let app = Expr {
            kind: ExprKind::App(Box::new(head), Box::new(x)),
            span,
        };
        Ok(Some(Expr {
            kind: ExprKind::App(Box::new(app), Box::new(lam)),
            span,
        }))
    }

    /// An operand of an operator: an application, or a keyword expression
    /// (`\`, `let`, `if`, `case`, `do`) extending to the right.
    fn operand(&mut self) -> R<Expr> {
        match self.peek() {
            Tok::Backslash | Tok::Kw(Kw::Let | Kw::If | Kw::Case | Kw::Do) => self.expr(),
            _ => self.app(),
        }
    }

    fn app(&mut self) -> R<Expr> {
        let start = self.span();
        let mut f = self.atom()?;
        while self.starts_atom() {
            let a = self.atom()?;
            f = Expr {
                kind: ExprKind::App(Box::new(f), Box::new(a)),
                span: self.from(start),
            };
        }
        // A trailing `\x => ...` / `do` block as the last argument is not
        // allowed (write `f $ \x => ...`).
        Ok(f)
    }

    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            Tok::LIdent(..)
                | Tok::UIdent(..)
                | Tok::Int(_)
                | Tok::Float(_)
                | Tok::Char(_)
                | Tok::Str(_)
                | Tok::Hole(_)
                | Tok::Underscore
                | Tok::LParen
                | Tok::LBracket
        )
    }

    fn atom(&mut self) -> R<Expr> {
        let start = self.span();
        let t = self.peek().clone();
        let kind = match t {
            Tok::LIdent(m, n) => {
                self.bump();
                ExprKind::Var(QName { module: m, name: n })
            }
            Tok::UIdent(m, n) => {
                self.bump();
                if m.is_empty() && n == "Type" {
                    ExprKind::Type
                } else if m.is_empty() && n == "LType" {
                    ExprKind::LType
                } else {
                    ExprKind::Var(QName { module: m, name: n })
                }
            }
            Tok::Int(n) => {
                self.bump();
                ExprKind::Lit(Lit::Int(n))
            }
            Tok::Float(x) => {
                self.bump();
                ExprKind::Lit(Lit::Float(x))
            }
            Tok::Char(c) => {
                self.bump();
                ExprKind::Lit(Lit::Char(c))
            }
            Tok::Str(s) => {
                self.bump();
                ExprKind::Lit(Lit::Str(s))
            }
            Tok::Hole(h) => {
                self.bump();
                ExprKind::Hole(h)
            }
            Tok::Underscore => {
                self.bump();
                ExprKind::Wildcard
            }
            Tok::LBracket => {
                self.bump();
                let mut items = Vec::new();
                if !self.at(&Tok::RBracket) {
                    items.push(self.expr()?);
                    while self.eat(&Tok::Comma) {
                        items.push(self.expr()?);
                    }
                }
                self.expect(&Tok::RBracket, "`]` or `,`")?;
                ExprKind::List(items)
            }
            Tok::LParen => return self.paren(),
            _ => {
                self.error_here("an expression");
                return Err(());
            }
        };
        Ok(Expr {
            kind,
            span: self.from(start),
        })
    }

    fn paren(&mut self) -> R<Expr> {
        let start = self.bump().span; // (
        if self.eat(&Tok::RParen) {
            return Ok(Expr {
                kind: ExprKind::Unit,
                span: self.from(start),
            });
        }
        // `(op)` and right sections `(op e)`.
        let is_neg = matches!(self.peek(), Tok::Op(p, n) if p.is_empty() && n == "-")
            && !matches!(self.peek_at(1), Tok::RParen);
        if !is_neg && matches!(self.peek(), Tok::Op(..)) {
            let t = self.bump();
            let Tok::Op(m, n) = t.tok else { unreachable!() };
            let o = OpRef {
                name: QName { module: m, name: n },
                span: t.span,
                backtick: false,
            };
            if self.eat(&Tok::RParen) {
                return Ok(Expr {
                    kind: ExprKind::Var(o.name),
                    span: self.from(start),
                });
            }
            let e = self.opseq()?;
            self.expect(&Tok::RParen, "`)`")?;
            return Ok(Expr {
                kind: ExprKind::RightSection(o, Box::new(e)),
                span: self.from(start),
            });
        }
        if self.at(&Tok::Backtick) {
            let o = self.backtick_op()?;
            let e = self.opseq()?;
            self.expect(&Tok::RParen, "`)`")?;
            return Ok(Expr {
                kind: ExprKind::RightSection(o, Box::new(e)),
                span: self.from(start),
            });
        }
        let e = self.expr()?;
        match self.peek().clone() {
            Tok::RParen => {
                self.bump();
                // Keep the parentheses' span.
                Ok(Expr {
                    kind: e.kind,
                    span: self.from(start),
                })
            }
            Tok::Comma => {
                let mut items = vec![e];
                while self.eat(&Tok::Comma) {
                    items.push(self.expr()?);
                }
                self.expect(&Tok::RParen, "`)` or `,`")?;
                Ok(Expr {
                    kind: ExprKind::Tuple(items),
                    span: self.from(start),
                })
            }
            Tok::Colon => {
                self.bump();
                let ty = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(Expr {
                    kind: ExprKind::Ann(Box::new(e), Box::new(ty)),
                    span: self.from(start),
                })
            }
            Tok::Op(m, n) => {
                let sp = self.bump().span;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(Expr {
                    kind: ExprKind::LeftSection(
                        Box::new(e),
                        OpRef {
                            name: QName { module: m, name: n },
                            span: sp,
                            backtick: false,
                        },
                    ),
                    span: self.from(start),
                })
            }
            Tok::Backtick => {
                let o = self.backtick_op()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(Expr {
                    kind: ExprKind::LeftSection(Box::new(e), o),
                    span: self.from(start),
                })
            }
            _ => {
                self.error_here("`)`");
                Err(())
            }
        }
    }

    fn lam_binder(&mut self) -> R<LamBinder> {
        let sp = self.span();
        match self.peek().clone() {
            Tok::LIdent(p, n) if p.is_empty() => {
                self.bump();
                Ok(LamBinder {
                    name: Some(n),
                    span: sp,
                    mult: None,
                    ty: None,
                })
            }
            Tok::Underscore => {
                self.bump();
                Ok(LamBinder {
                    name: None,
                    span: sp,
                    mult: None,
                    ty: None,
                })
            }
            Tok::LParen => {
                self.bump();
                let mult = self.opt_mult();
                let name = match self.peek().clone() {
                    Tok::LIdent(p, n) if p.is_empty() => {
                        self.bump();
                        Some(n)
                    }
                    Tok::Underscore => {
                        self.bump();
                        None
                    }
                    _ => {
                        self.error_here("a variable name");
                        return Err(());
                    }
                };
                self.expect(&Tok::Colon, "`:`")?;
                let ty = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(LamBinder {
                    name,
                    span: self.from(sp),
                    mult,
                    ty: Some(ty),
                })
            }
            _ => {
                self.error_here("a lambda parameter");
                Err(())
            }
        }
    }

    fn let_bind(&mut self) -> R<LetBind> {
        let mult = self.opt_mult();
        let name_span = self.span();
        let name = match self.peek().clone() {
            Tok::LIdent(p, n) if p.is_empty() => {
                self.bump();
                n
            }
            _ => {
                self.error_here("a variable name");
                return Err(());
            }
        };
        let ty = if self.eat(&Tok::Colon) {
            Some(self.expr()?)
        } else {
            None
        };
        let mut args = Vec::new();
        if ty.is_none() {
            while self.starts_apat() {
                args.push(self.apat()?);
            }
        }
        self.expect(&Tok::Equals, "`=`")?;
        let rhs = self.expr()?;
        Ok(LetBind {
            mult,
            name,
            name_span,
            ty,
            args,
            rhs,
        })
    }

    fn stmt(&mut self) -> R<Stmt> {
        if let (Tok::LIdent(p, n), Tok::LArrow) = (self.peek().clone(), self.peek_at(1))
            && p.is_empty()
        {
            let sp = self.bump().span;
            self.bump();
            let e = self.expr()?;
            if self.at(&Tok::Question) {
                let q = self.bump().span;
                return Ok(Stmt::Try(Some((n, sp)), e, q));
            }
            return Ok(Stmt::Bind(n, sp, e));
        }
        // `p <- e` with a pattern: tried, and undone when no `<-` follows.
        if self.starts_apat() {
            let save = (self.pos, self.errs.len());
            if let Ok(p) = self.pat()
                && self.at(&Tok::LArrow)
            {
                self.bump();
                let e = self.expr()?;
                let mut alts = Vec::new();
                while self.eat(&Tok::Bar) {
                    let q = self.pat()?;
                    self.expect(&Tok::FatArrow, "`=>`")?;
                    let rhs = self.expr()?;
                    alts.push(Alt { pat: q, rhs });
                }
                return Ok(Stmt::BindPat(p, e, alts));
            }
            self.pos = save.0;
            self.errs.truncate(save.1);
        }
        if self.at_kw(Kw::Let) {
            let start = self.span();
            self.bump();
            let bs = self.block(|p| p.let_bind())?;
            if self.at_kw(Kw::In) {
                self.bump();
                let body = self.expr()?;
                return Ok(Stmt::Expr(Expr {
                    kind: ExprKind::Let(bs, Box::new(body)),
                    span: self.from(start),
                }));
            }
            return Ok(Stmt::Let(bs));
        }
        let e = self.expr()?;
        if self.at(&Tok::Question) {
            let q = self.bump().span;
            return Ok(Stmt::Try(None, e, q));
        }
        Ok(Stmt::Expr(e))
    }

    // ---- patterns ----

    fn starts_apat(&self) -> bool {
        match self.peek() {
            Tok::LIdent(p, _) => p.is_empty(),
            Tok::UIdent(..)
            | Tok::Underscore
            | Tok::Int(_)
            | Tok::Float(_)
            | Tok::Char(_)
            | Tok::Str(_)
            | Tok::LParen
            | Tok::LBracket => true,
            Tok::Op(p, n) => {
                p.is_empty() && n == "-" && matches!(self.peek_at(1), Tok::Int(_) | Tok::Float(_))
            }
            _ => false,
        }
    }

    pub(crate) fn pat(&mut self) -> R<Pat> {
        let start = self.span();
        let first = self.con_app_pat()?;
        let mut pats = vec![first];
        let mut ops = Vec::new();
        loop {
            match self.peek().clone() {
                Tok::Op(m, n) if n.starts_with(':') => {
                    let sp = self.bump().span;
                    ops.push(OpRef {
                        name: QName { module: m, name: n },
                        span: sp,
                        backtick: false,
                    });
                }
                Tok::Backtick if matches!(self.peek_at(1), Tok::UIdent(..)) => {
                    ops.push(self.backtick_op()?);
                }
                _ => break,
            }
            pats.push(self.con_app_pat()?);
        }
        if ops.is_empty() {
            return Ok(pats.pop().unwrap());
        }
        Ok(Pat {
            kind: PatKind::OpSeq(pats, ops),
            span: self.from(start),
        })
    }

    fn con_app_pat(&mut self) -> R<Pat> {
        let start = self.span();
        if let Tok::UIdent(m, n) = self.peek().clone() {
            self.bump();
            let mut args = Vec::new();
            while self.starts_apat() {
                args.push(self.apat()?);
            }
            return Ok(Pat {
                kind: PatKind::Con(QName { module: m, name: n }, args),
                span: self.from(start),
            });
        }
        self.apat()
    }

    fn apat(&mut self) -> R<Pat> {
        let start = self.span();
        let kind = match self.peek().clone() {
            Tok::LIdent(p, n) if p.is_empty() => {
                self.bump();
                PatKind::Var(n)
            }
            Tok::Underscore => {
                self.bump();
                PatKind::Wild
            }
            Tok::UIdent(m, n) => {
                self.bump();
                PatKind::Con(QName { module: m, name: n }, Vec::new())
            }
            Tok::Int(n) => {
                self.bump();
                PatKind::Lit(Lit::Int(n), false)
            }
            Tok::Float(x) => {
                self.bump();
                PatKind::Lit(Lit::Float(x), false)
            }
            Tok::Char(c) => {
                self.bump();
                PatKind::Lit(Lit::Char(c), false)
            }
            Tok::Str(s) => {
                self.bump();
                PatKind::Lit(Lit::Str(s), false)
            }
            Tok::Op(p, n) if p.is_empty() && n == "-" => {
                self.bump();
                match self.peek().clone() {
                    Tok::Int(k) => {
                        self.bump();
                        PatKind::Lit(Lit::Int(k), true)
                    }
                    Tok::Float(x) => {
                        self.bump();
                        PatKind::Lit(Lit::Float(-x), false)
                    }
                    _ => {
                        self.error_here("a number");
                        return Err(());
                    }
                }
            }
            Tok::LParen => {
                self.bump();
                if self.eat(&Tok::RParen) {
                    PatKind::Unit
                } else {
                    let p = self.pat()?;
                    if self.at(&Tok::Comma) {
                        let mut items = vec![p];
                        while self.eat(&Tok::Comma) {
                            items.push(self.pat()?);
                        }
                        self.expect(&Tok::RParen, "`)` or `,`")?;
                        PatKind::Tuple(items)
                    } else {
                        self.expect(&Tok::RParen, "`)`")?;
                        p.kind
                    }
                }
            }
            Tok::LBracket => {
                self.bump();
                let mut items = Vec::new();
                if !self.at(&Tok::RBracket) {
                    items.push(self.pat()?);
                    while self.eat(&Tok::Comma) {
                        items.push(self.pat()?);
                    }
                }
                self.expect(&Tok::RBracket, "`]` or `,`")?;
                PatKind::List(items)
            }
            _ => {
                self.error_here("a pattern");
                return Err(());
            }
        };
        Ok(Pat {
            kind,
            span: self.from(start),
        })
    }
}
