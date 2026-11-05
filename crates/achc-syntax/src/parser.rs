//! Recursive-descent parser for Haskell 2010 (Report §3–5) with the layout
//! algorithm (Report §10.3) driven by the parser, so that the
//! `parse-error(t)` rule can close implicit blocks.
// Errors are collected in the parser; `Err(())` only unwinds.
#![allow(clippy::result_unit_err)]

use crate::ast::*;
use crate::lexer::{Kw, Tok, Token};
use cirka_diagnostics::{Diagnostic, Span};

#[derive(Clone, Debug, PartialEq)]
enum Ctx {
    Implicit(u32),
    Explicit,
}

enum Block {
    Explicit,
    Implicit,
    Empty,
}

/// Parser state that can be saved and restored for backtracking.
#[derive(Clone)]
struct State {
    pos: usize,
    stack: Vec<Ctx>,
    semi_done: usize,
    block_start: usize,
    nerrors: usize,
}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    stack: Vec<Ctx>,
    /// The token at this position has had its layout `;` consumed.
    semi_done: usize,
    /// First token of the most recently opened implicit block (no `;`
    /// before it).
    block_start: usize,
    errors: Vec<Diagnostic>,
    /// Nesting of parse attempts; errors inside are discarded on failure.
    fuel: u32,
    /// `StrictData` (or `Strict`): constructor fields are strict unless
    /// marked lazy with `~`.
    strict_data: bool,
    /// `Strict`: arguments, case alternatives and local bindings are
    /// strict unless marked lazy with `~`.
    strict: bool,
    /// Nesting of `let`/`where` blocks.
    local_depth: u32,
}

type PResult<T> = Result<T, ()>;

impl Parser {
    pub fn new(toks: Vec<Token>) -> Parser {
        Parser {
            toks,
            pos: 0,
            stack: Vec::new(),
            semi_done: usize::MAX,
            block_start: usize::MAX,
            errors: Vec::new(),
            fuel: 0,
            strict_data: false,
            strict: false,
            local_depth: 0,
        }
    }

    // ---- tokens and layout ----

    fn real(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn first_on_line(&self, pos: usize) -> bool {
        pos == 0 || self.toks[pos - 1].line != self.toks[pos].line
    }

    /// The current token, including virtual layout tokens.
    fn cur(&self) -> Tok {
        let t = self.real();
        if let Some(Ctx::Implicit(m)) = self.stack.last() {
            if t.tok == Tok::Eof {
                return Tok::VRBrace;
            }
            if self.first_on_line(self.pos) && self.pos != self.block_start {
                if t.col < *m {
                    return Tok::VRBrace;
                }
                if t.col == *m && self.semi_done != self.pos {
                    return Tok::VSemi;
                }
            }
        }
        t.tok.clone()
    }

    /// The real token after the current one (no layout).
    fn peek_real(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        let t = self.real();
        match self.cur() {
            Tok::VSemi | Tok::VRBrace => Span::new(t.span.file, t.span.start, t.span.start),
            _ => t.span,
        }
    }

    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            return self.toks[0].span;
        }
        self.toks[self.pos - 1].span
    }

    fn bump(&mut self) {
        match self.cur() {
            Tok::VRBrace => {
                self.stack.pop();
            }
            Tok::VSemi => self.semi_done = self.pos,
            Tok::Eof => {}
            _ => self.pos += 1,
        }
    }

    fn at(&self, t: &Tok) -> bool {
        &self.cur() == t
    }

    fn at_kw(&self, k: Kw) -> bool {
        self.cur() == Tok::Kw(k)
    }

    fn at_varsym(&self, s: &str) -> bool {
        matches!(self.cur(), Tok::VarSym(None, ref x) if x == s)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, k: Kw) -> bool {
        self.eat(&Tok::Kw(k))
    }

    fn save(&self) -> State {
        State {
            pos: self.pos,
            stack: self.stack.clone(),
            semi_done: self.semi_done,
            block_start: self.block_start,
            nerrors: self.errors.len(),
        }
    }

    fn restore(&mut self, s: State) {
        self.pos = s.pos;
        self.stack = s.stack;
        self.semi_done = s.semi_done;
        self.block_start = s.block_start;
        self.errors.truncate(s.nerrors);
    }

    fn error_at(&mut self, span: Span, msg: String, label: String) {
        if self.fuel > 0 {
            return;
        }
        // One error per position is enough.
        if self
            .errors
            .last()
            .is_some_and(|d| d.primary_span() == Some(span))
        {
            return;
        }
        self.errors.push(
            Diagnostic::error(msg)
                .with_code("E0102")
                .with_label(span, label),
        );
    }

    fn expected<T>(&mut self, what: &str) -> PResult<T> {
        let sp = self.span();
        let found = self.cur().describe();
        if self.fuel == 0 {
            self.error_at(
                sp,
                format!("expected {what}, found {found}"),
                format!("expected {what}"),
            );
        }
        Err(())
    }

    fn expect(&mut self, t: &Tok) -> PResult<()> {
        if self.eat(t) {
            Ok(())
        } else {
            self.expected(&t.describe())
        }
    }

    /// Runs `f`; on failure restores the state and returns `None` without
    /// reporting errors.
    fn attempt<T>(&mut self, f: impl FnOnce(&mut Parser) -> PResult<T>) -> Option<T> {
        let s = self.save();
        self.fuel += 1;
        let r = f(self);
        self.fuel -= 1;
        match r {
            Ok(v) => Some(v),
            Err(()) => {
                self.restore(s);
                None
            }
        }
    }

    fn open_block(&mut self) -> Block {
        if self.real().tok == Tok::LBrace && !matches!(self.cur(), Tok::VRBrace | Tok::VSemi) {
            self.pos += 1;
            self.stack.push(Ctx::Explicit);
            return Block::Explicit;
        }
        let t = self.real();
        let n = if t.tok == Tok::Eof { 0 } else { t.col };
        let m = match self.stack.last() {
            Some(Ctx::Implicit(m)) => *m,
            _ => 0,
        };
        if n > m {
            self.stack.push(Ctx::Implicit(n));
            self.block_start = self.pos;
            Block::Implicit
        } else {
            Block::Empty
        }
    }

    /// Parses `{ item; ...; item }` with layout. Errors inside an item skip
    /// to the next item.
    fn block<T>(&mut self, mut item: impl FnMut(&mut Parser) -> PResult<T>) -> Vec<T> {
        let mut out = Vec::new();
        match self.open_block() {
            Block::Empty => {}
            Block::Explicit => loop {
                while self.eat(&Tok::Semi) {}
                if self.eat(&Tok::RBrace) {
                    self.stack.pop();
                    break;
                }
                if self.at(&Tok::Eof) {
                    let _ = self.expected::<()>("`}`");
                    self.stack.pop();
                    break;
                }
                match item(self) {
                    Ok(x) => out.push(x),
                    Err(()) => self.skip_item(),
                }
                if self.eat(&Tok::Semi) {
                    continue;
                }
                if self.eat(&Tok::RBrace) {
                    self.stack.pop();
                    break;
                }
                let _ = self.expected::<()>("`;` or `}`");
                self.skip_item();
                if self.eat(&Tok::RBrace) {
                    self.stack.pop();
                    break;
                }
                if self.at(&Tok::Eof) {
                    self.stack.pop();
                    break;
                }
            },
            Block::Implicit => {
                let depth = self.stack.len();
                loop {
                    while matches!(self.cur(), Tok::VSemi | Tok::Semi) {
                        self.bump();
                    }
                    if self.at(&Tok::VRBrace) {
                        self.bump();
                        break;
                    }
                    if !can_start_item(&self.cur()) {
                        // parse-error(t) before any item.
                        self.stack.truncate(depth - 1);
                        break;
                    }
                    let before = self.pos;
                    match item(self) {
                        Ok(x) => out.push(x),
                        Err(()) => {
                            self.skip_item();
                            if self.pos == before
                                && !matches!(self.cur(), Tok::VSemi | Tok::VRBrace)
                            {
                                // Nothing could be parsed: parse-error(t).
                                self.stack.truncate(depth - 1);
                                break;
                            }
                        }
                    }
                    // Inner blocks closed by parse-error(t) may have left
                    // their contexts; keep the stack consistent.
                    if self.stack.len() < depth {
                        break;
                    }
                    match self.cur() {
                        Tok::VSemi | Tok::Semi => continue,
                        Tok::VRBrace => {
                            self.bump();
                            break;
                        }
                        _ => {
                            // parse-error(t): the token cannot continue the
                            // block, so the implicit block ends here.
                            self.stack.truncate(depth - 1);
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    /// Skips to the end of the current block item.
    fn skip_item(&mut self) {
        let depth = self.stack.len();
        let mut nest = 0i32;
        loop {
            match self.cur() {
                Tok::Eof => return,
                Tok::VSemi | Tok::VRBrace if self.stack.len() <= depth => return,
                Tok::Semi | Tok::RBrace if nest == 0 && self.stack.len() <= depth => return,
                Tok::LParen | Tok::LBracket | Tok::LBrace => nest += 1,
                Tok::RParen | Tok::RBracket | Tok::RBrace => nest -= 1,
                _ => {}
            }
            let before = (self.pos, self.stack.len(), self.semi_done);
            self.bump();
            if (self.pos, self.stack.len(), self.semi_done) == before {
                return;
            }
        }
    }

    // ---- names ----

    fn varid(&mut self) -> PResult<Ident> {
        match self.cur() {
            Tok::VarId(None, n) => {
                let span = self.span();
                self.bump();
                Ok(Ident { name: n, span })
            }
            _ => self.expected("a variable name"),
        }
    }

    fn conid(&mut self) -> PResult<Ident> {
        match self.cur() {
            Tok::ConId(None, n) => {
                let span = self.span();
                self.bump();
                Ok(Ident { name: n, span })
            }
            _ => self.expected("a constructor or type name"),
        }
    }

    /// A module name `A.B.C` (lexed as a qualified constructor).
    fn modid(&mut self) -> PResult<Ident> {
        match self.cur() {
            Tok::ConId(q, n) => {
                let span = self.span();
                self.bump();
                let name = match q {
                    Some(q) => format!("{q}.{n}"),
                    None => n,
                };
                Ok(Ident { name, span })
            }
            _ => self.expected("a module name"),
        }
    }

    /// `var` or `( varsym )`; also constructors when `con` is set.
    fn var_or_parens_op(&mut self, con: bool) -> PResult<Ident> {
        match self.cur() {
            Tok::VarId(None, n) => {
                let span = self.span();
                self.bump();
                Ok(Ident { name: n, span })
            }
            Tok::ConId(None, n) if con => {
                let span = self.span();
                self.bump();
                Ok(Ident { name: n, span })
            }
            Tok::LParen => {
                let start = self.span();
                self.bump();
                let name = match self.cur() {
                    Tok::VarSym(None, s) => s,
                    Tok::ConSym(None, s) if con => s,
                    _ => return self.expected("an operator"),
                };
                self.bump();
                self.expect(&Tok::RParen)?;
                Ok(Ident {
                    name,
                    span: start.to(self.prev_span()),
                })
            }
            _ => self.expected("a name"),
        }
    }

    /// An operator in infix position: a symbol or a back-quoted name.
    fn qop(&self) -> Option<(QName, usize)> {
        let t = self.real();
        if matches!(self.cur(), Tok::VSemi | Tok::VRBrace) {
            return None;
        }
        match &t.tok {
            Tok::VarSym(q, s) | Tok::ConSym(q, s) => Some((
                QName {
                    qual: q.clone(),
                    name: s.clone(),
                    span: t.span,
                },
                1,
            )),
            Tok::Backtick => {
                let n = &self.toks[(self.pos + 1).min(self.toks.len() - 1)];
                let close = &self.toks[(self.pos + 2).min(self.toks.len() - 1)];
                match (&n.tok, &close.tok) {
                    (Tok::VarId(q, s) | Tok::ConId(q, s), Tok::Backtick) => Some((
                        QName {
                            qual: q.clone(),
                            name: s.clone(),
                            span: t.span.to(close.span),
                        },
                        3,
                    )),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    // ---- module ----

    pub fn module(&mut self, extensions: Vec<(String, Span)>) -> Module {
        self.strict_data = extensions
            .iter()
            .any(|(e, _)| e == "StrictData" || e == "Strict");
        self.strict = extensions.iter().any(|(e, _)| e == "Strict");
        let start = self.span();
        let mut name = None;
        let mut exports = None;
        if self.at_kw(Kw::Module) {
            self.bump();
            if let Ok(n) = self.modid() {
                name = Some(n);
            }
            if self.at(&Tok::LParen) {
                exports = self.exports().ok();
            }
            if self.expect(&Tok::Kw(Kw::Where)).is_err() {
                self.skip_to_where();
            }
        }
        let mut imports = Vec::new();
        let mut decls = Vec::new();
        let items = self.block(|p| {
            if p.at_kw(Kw::Import) {
                p.import().map(TopItem::Import)
            } else {
                p.topdecl().map(TopItem::Decl)
            }
        });
        for it in items {
            match it {
                TopItem::Import(i) => imports.push(i),
                TopItem::Decl(d) => decls.extend(d),
            }
        }
        if !self.at(&Tok::Eof) {
            let _ = self.expected::<()>("a declaration");
        }
        Module {
            name,
            exports,
            imports,
            decls,
            extensions,
            span: start.to(self.prev_span()),
        }
    }

    fn skip_to_where(&mut self) {
        while !matches!(self.real().tok, Tok::Eof | Tok::Kw(Kw::Where)) {
            self.pos += 1;
        }
        if self.real().tok == Tok::Kw(Kw::Where) {
            self.pos += 1;
        }
    }

    fn exports(&mut self) -> PResult<Vec<Export>> {
        self.expect(&Tok::LParen)?;
        let mut out = Vec::new();
        loop {
            if self.eat(&Tok::RParen) {
                break;
            }
            if self.eat_kw(Kw::Module) {
                out.push(Export::Module(self.modid()?));
            } else {
                match self.cur() {
                    Tok::ConId(q, n) => {
                        let span = self.span();
                        self.bump();
                        let items = self.item_list()?;
                        out.push(Export::Thing(
                            QName {
                                qual: q,
                                name: n,
                                span,
                            },
                            items,
                        ));
                    }
                    Tok::VarId(q, n) => {
                        let span = self.span();
                        self.bump();
                        out.push(Export::Var(QName {
                            qual: q,
                            name: n,
                            span,
                        }));
                    }
                    Tok::LParen => {
                        let start = self.span();
                        self.bump();
                        let (q, n, con) = match self.cur() {
                            Tok::VarSym(q, s) => (q, s, false),
                            Tok::ConSym(q, s) => (q, s, true),
                            _ => return self.expected("an operator"),
                        };
                        self.bump();
                        self.expect(&Tok::RParen)?;
                        let qn = QName {
                            qual: q,
                            name: n,
                            span: start.to(self.prev_span()),
                        };
                        if con {
                            let items = self.item_list()?;
                            out.push(Export::Thing(qn, items));
                        } else {
                            out.push(Export::Var(qn));
                        }
                    }
                    _ => return self.expected("an export"),
                }
            }
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RParen)?;
                break;
            }
        }
        Ok(out)
    }

    /// `(..)` or `(a, B, (+))` after a type or class name.
    fn item_list(&mut self) -> PResult<Option<ItemList>> {
        if !self.at(&Tok::LParen) {
            return Ok(None);
        }
        self.bump();
        if self.eat(&Tok::DotDot) {
            self.expect(&Tok::RParen)?;
            return Ok(Some(ItemList::All));
        }
        let mut items = Vec::new();
        while !self.eat(&Tok::RParen) {
            items.push(self.var_or_parens_op(true)?);
            if !self.eat(&Tok::Comma) {
                self.expect(&Tok::RParen)?;
                break;
            }
        }
        Ok(Some(ItemList::Some(items)))
    }

    fn import(&mut self) -> PResult<Import> {
        let start = self.span();
        self.bump();
        let special = |p: &Parser, s: &str| matches!(p.cur(), Tok::VarId(None, ref n) if n == s);
        let mut qualified = false;
        if special(self, "qualified") {
            self.bump();
            qualified = true;
        }
        let module = self.modid()?;
        // `import M qualified` (ImportQualifiedPost) is not Haskell 2010.
        let mut alias = None;
        if special(self, "as") {
            self.bump();
            alias = Some(self.modid()?);
        }
        let mut hiding = false;
        if special(self, "hiding") {
            self.bump();
            hiding = true;
        }
        let mut items = None;
        if self.at(&Tok::LParen) {
            self.bump();
            let mut v = Vec::new();
            loop {
                if self.eat(&Tok::RParen) {
                    break;
                }
                match self.cur() {
                    Tok::ConId(None, _) => {
                        let n = self.conid()?;
                        let l = self.item_list()?;
                        v.push(ImportItem::Thing(n, l));
                    }
                    Tok::LParen if matches!(self.peek_real(1), Tok::ConSym(..)) => {
                        let n = self.var_or_parens_op(true)?;
                        let l = self.item_list()?;
                        v.push(ImportItem::Thing(n, l));
                    }
                    _ => v.push(ImportItem::Var(self.var_or_parens_op(false)?)),
                }
                if !self.eat(&Tok::Comma) {
                    self.expect(&Tok::RParen)?;
                    break;
                }
            }
            items = Some(v);
        }
        Ok(Import {
            module,
            qualified,
            alias,
            hiding,
            items,
            span: start.to(self.prev_span()),
        })
    }

    // ---- declarations ----

    fn topdecl(&mut self) -> PResult<Vec<Decl>> {
        let start = self.span();
        match self.cur() {
            Tok::Kw(Kw::Data) | Tok::Kw(Kw::Newtype) => {
                let newtype = self.at_kw(Kw::Newtype);
                self.bump();
                Ok(vec![Decl::Data(self.data_decl(newtype, start)?)])
            }
            Tok::Kw(Kw::Type) => {
                self.bump();
                let name = self.conid()?;
                let params = self.tyvars();
                self.expect(&Tok::Equals)?;
                let ty = self.ty()?;
                Ok(vec![Decl::TypeSyn(
                    name,
                    params,
                    ty,
                    start.to(self.prev_span()),
                )])
            }
            Tok::Kw(Kw::Class) => {
                self.bump();
                let (context, head) = self.context_and_head()?;
                let (name, params, param_kinds) = match head.kind {
                    TypeKind::App(..) | TypeKind::Con(_) => {
                        let (h, args) = flatten_tyapp(&head);
                        let TypeKind::Con(c) = &h.kind else {
                            return self.expected("a class name");
                        };
                        let mut ps = Vec::new();
                        let mut pks = Vec::new();
                        for a in args {
                            match tyvar_binder(a) {
                                Some((v, k)) => {
                                    ps.push(v);
                                    pks.push(k);
                                }
                                None => {
                                    self.error_at(
                                        a.span,
                                        "class parameters must be type variables".into(),
                                        "expected a type variable".into(),
                                    );
                                    return Err(());
                                }
                            }
                        }
                        (
                            Ident {
                                name: c.name.clone(),
                                span: c.span,
                            },
                            ps,
                            pks,
                        )
                    }
                    _ => return self.expected("a class name"),
                };
                let decls = if self.eat_kw(Kw::Where) {
                    self.decls_block()
                } else {
                    Vec::new()
                };
                Ok(vec![Decl::Class(ClassDecl {
                    context,
                    name,
                    params,
                    param_kinds,
                    decls,
                    span: start.to(self.prev_span()),
                })])
            }
            Tok::Kw(Kw::Instance) => {
                self.bump();
                let (context, head) = self.context_and_head()?;
                let (h, args) = flatten_tyapp(&head);
                let TypeKind::Con(class) = h.kind.clone() else {
                    return self.expected("a class name");
                };
                let decls = if self.eat_kw(Kw::Where) {
                    self.decls_block()
                } else {
                    Vec::new()
                };
                Ok(vec![Decl::Instance(InstDecl {
                    context,
                    class,
                    types: args.into_iter().cloned().collect(),
                    decls,
                    span: start.to(self.prev_span()),
                })])
            }
            Tok::Kw(Kw::Default) => {
                self.bump();
                self.expect(&Tok::LParen)?;
                let mut tys = Vec::new();
                while !self.eat(&Tok::RParen) {
                    tys.push(self.ty()?);
                    if !self.eat(&Tok::Comma) {
                        self.expect(&Tok::RParen)?;
                        break;
                    }
                }
                Ok(vec![Decl::Default(tys, start.to(self.prev_span()))])
            }
            Tok::Kw(Kw::Foreign) => {
                self.bump();
                let import = match self.cur() {
                    Tok::Kw(Kw::Import) => true,
                    Tok::VarId(None, ref s) if s == "export" => false,
                    _ => return self.expected("`import` or `export`"),
                };
                self.bump();
                let callconv = self.varid()?;
                if matches!(self.cur(), Tok::VarId(None, ref s) if s == "safe" || s == "unsafe") {
                    self.bump();
                }
                let entity = match self.cur() {
                    Tok::Str(s) => {
                        self.bump();
                        Some(s)
                    }
                    _ => None,
                };
                let name = self.var_or_parens_op(false)?;
                self.expect(&Tok::DColon)?;
                let ty = self.ty()?;
                Ok(vec![Decl::Foreign(ForeignDecl {
                    import,
                    callconv,
                    entity,
                    name,
                    ty,
                    span: start.to(self.prev_span()),
                })])
            }
            _ => self.decl(),
        }
    }

    fn decls_block(&mut self) -> Vec<Decl> {
        self.block(|p| p.decl()).into_iter().flatten().collect()
    }

    /// The declarations of a `let` or `where`.
    fn local_decls_block(&mut self) -> Vec<Decl> {
        self.local_depth += 1;
        let ds = self.decls_block();
        self.local_depth -= 1;
        ds
    }

    /// Under `Strict`, a pattern without `!` or `~` is strict.
    fn strict_pat(&self, p: Pat) -> Pat {
        if !self.strict || matches!(p.kind, PatKind::Bang(_) | PatKind::Lazy(_)) {
            return p;
        }
        let span = p.span;
        Pat {
            kind: PatKind::Bang(Box::new(p)),
            span,
        }
    }

    /// `Strict` applied to a declaration: strict arguments, and strict
    /// local variable bindings (`x = e` is `!x = e`).
    fn strict_decl(&self, d: Decl) -> Decl {
        if !self.strict {
            return d;
        }
        match d {
            Decl::Fun(f, args, rhs, sp) if args.is_empty() && self.local_depth > 0 => {
                let v = Pat {
                    kind: PatKind::Var(f.clone()),
                    span: f.span,
                };
                Decl::PatBind(self.strict_pat(v), rhs, sp)
            }
            Decl::Fun(f, args, rhs, sp) => Decl::Fun(
                f,
                args.into_iter().map(|a| self.strict_pat(a)).collect(),
                rhs,
                sp,
            ),
            Decl::PatBind(p, rhs, sp) if self.local_depth > 0 => {
                Decl::PatBind(self.strict_pat(p), rhs, sp)
            }
            d => d,
        }
    }

    /// Type variable binders; `(v :: k)` binders drop their kind.
    fn tyvars(&mut self) -> Vec<Ident> {
        let mut v = Vec::new();
        loop {
            match self.cur() {
                Tok::VarId(None, n) => {
                    v.push(Ident {
                        name: n,
                        span: self.span(),
                    });
                    self.bump();
                }
                Tok::LParen
                    if matches!(self.peek_real(1), Tok::VarId(None, _))
                        && self.peek_real(2) == &Tok::DColon =>
                {
                    self.bump();
                    let Tok::VarId(None, n) = self.cur() else {
                        unreachable!()
                    };
                    v.push(Ident {
                        name: n,
                        span: self.span(),
                    });
                    self.bump();
                    self.bump();
                    if self.kind_sig().is_err() || !self.eat(&Tok::RParen) {
                        break;
                    }
                }
                _ => break,
            }
        }
        v
    }

    /// A kind (KindSignatures): `*`, `Type`, `k -> k`, `(k)`.
    fn kind_sig(&mut self) -> PResult<KindSig> {
        let a = match self.cur() {
            Tok::VarSym(None, ref s) if s == "*" => {
                self.bump();
                KindSig::Star
            }
            Tok::ConId(_, ref n) if n == "Type" => {
                self.bump();
                KindSig::Star
            }
            Tok::LParen => {
                self.bump();
                let k = self.kind_sig()?;
                self.expect(&Tok::RParen)?;
                k
            }
            _ => return self.expected("a kind (`*`, `Type` or `k -> k`)"),
        };
        if self.eat(&Tok::RArrow) {
            let r = self.kind_sig()?;
            return Ok(KindSig::Arrow(Box::new(a), Box::new(r)));
        }
        Ok(a)
    }

    fn data_decl(&mut self, newtype: bool, start: Span) -> PResult<DataDecl> {
        let (context, head) = self.context_and_head()?;
        let (h, args) = flatten_tyapp(&head);
        let TypeKind::Con(c) = &h.kind else {
            return self.expected("a type name");
        };
        let name = Ident {
            name: c.name.clone(),
            span: c.span,
        };
        let mut params = Vec::new();
        let mut param_kinds = Vec::new();
        for a in args {
            match tyvar_binder(a) {
                Some((v, k)) => {
                    params.push(v);
                    param_kinds.push(k);
                }
                None => {
                    self.error_at(
                        a.span,
                        "type parameters must be type variables".into(),
                        "expected a type variable".into(),
                    );
                    return Err(());
                }
            }
        }
        let mut cons = Vec::new();
        if self.eat(&Tok::Equals) {
            loop {
                cons.push(self.con_decl()?);
                if !self.eat(&Tok::Bar) {
                    break;
                }
            }
        }
        let mut deriving = Vec::new();
        if self.eat_kw(Kw::Deriving) {
            let one = |p: &mut Parser| -> PResult<QName> {
                match p.cur() {
                    Tok::ConId(q, n) => {
                        let span = p.span();
                        p.bump();
                        Ok(QName {
                            qual: q,
                            name: n,
                            span,
                        })
                    }
                    _ => p.expected("a class name"),
                }
            };
            if self.eat(&Tok::LParen) {
                while !self.eat(&Tok::RParen) {
                    deriving.push(one(self)?);
                    if !self.eat(&Tok::Comma) {
                        self.expect(&Tok::RParen)?;
                        break;
                    }
                }
            } else {
                deriving.push(one(self)?);
            }
        }
        Ok(DataDecl {
            newtype,
            context,
            name,
            params,
            param_kinds,
            cons,
            deriving,
            span: start.to(self.prev_span()),
        })
    }

    /// A strictness annotation of a constructor field: `!` (strict), `~`
    /// (lazy) or none (strict under `StrictData`).
    fn field_strictness(&mut self) -> Option<bool> {
        if self.at_varsym("!") {
            self.bump();
            Some(true)
        } else if self.at(&Tok::Tilde) {
            self.bump();
            Some(false)
        } else {
            None
        }
    }

    fn strict_atype(&mut self) -> PResult<(Type, bool)> {
        let s = self.field_strictness();
        Ok((self.atype()?, s.unwrap_or(self.strict_data)))
    }

    fn at_atype_start(&self) -> bool {
        matches!(
            self.cur(),
            Tok::VarId(None, _) | Tok::ConId(..) | Tok::LParen | Tok::LBracket
        ) || self.at_varsym("!")
            || self.at(&Tok::Tilde)
    }

    fn con_decl(&mut self) -> PResult<ConDecl> {
        let start = self.span();
        // Record syntax: `C { f1, f2 :: T, ... }`.
        if let Tok::ConId(None, _) = self.cur()
            && self.peek_real(1) == &Tok::LBrace
        {
            let name = self.conid()?;
            self.expect(&Tok::LBrace)?;
            let mut fields = Vec::new();
            while !self.eat(&Tok::RBrace) {
                let mut names = vec![self.var_or_parens_op(false)?];
                while self.eat(&Tok::Comma) {
                    names.push(self.var_or_parens_op(false)?);
                }
                self.expect(&Tok::DColon)?;
                let (ty, strict) = match self.field_strictness() {
                    Some(s) => (self.atype()?, s),
                    None => (self.ty()?, self.strict_data),
                };
                fields.push((names, ty, strict));
                if !self.eat(&Tok::Comma) {
                    self.expect(&Tok::RBrace)?;
                    break;
                }
            }
            return Ok(ConDecl {
                name,
                fields: ConFields::Record(fields),
                span: start.to(self.prev_span()),
            });
        }
        // `(:+) a b` or `C a b`.
        let prefix_name = match self.cur() {
            Tok::ConId(None, n) => {
                let sp = self.span();
                Some(Ident { name: n, span: sp })
            }
            Tok::LParen if matches!(self.peek_real(1), Tok::ConSym(None, _)) => None,
            _ => None,
        };
        let mut items = Vec::new();
        let paren_op = if prefix_name.is_none() && self.at(&Tok::LParen) {
            if let Tok::ConSym(None, _) = self.peek_real(1) {
                Some(self.var_or_parens_op(true)?)
            } else {
                None
            }
        } else {
            None
        };
        if prefix_name.is_some() {
            self.bump();
        }
        while self.at_atype_start() {
            items.push(self.strict_atype()?);
        }
        // Infix constructor: `t1 :+ t2` or ``t1 `C` t2``.
        if let Some((op, n)) = self.qop()
            && op.is_con()
            && paren_op.is_none()
        {
            let left = match prefix_name {
                Some(c) => {
                    // The left operand is a type application `C t ..`.
                    let mut t = Type {
                        kind: TypeKind::Con(QName::unqual(c.name.clone(), c.span)),
                        span: c.span,
                    };
                    let mut strict = false;
                    for (a, s) in items {
                        strict |= s;
                        let sp = t.span.to(a.span);
                        t = Type {
                            kind: TypeKind::App(Box::new(t), Box::new(a)),
                            span: sp,
                        };
                    }
                    (t, strict)
                }
                None if items.len() == 1 => items.pop().unwrap(),
                None => return self.expected("a constructor"),
            };
            self.bump_n(n);
            let right = match self.field_strictness() {
                Some(s) => (self.atype()?, s),
                None => (self.btype()?, self.strict_data),
            };
            return Ok(ConDecl {
                name: Ident {
                    name: op.name,
                    span: op.span,
                },
                fields: ConFields::Plain(vec![left, right]),
                span: start.to(self.prev_span()),
            });
        }
        let name = match (prefix_name, paren_op) {
            (Some(n), _) | (None, Some(n)) => n,
            _ => return self.expected("a constructor"),
        };
        Ok(ConDecl {
            name,
            fields: ConFields::Plain(items),
            span: start.to(self.prev_span()),
        })
    }

    /// `[context =>] head` for class, instance and data declarations.
    fn context_and_head(&mut self) -> PResult<(Vec<Constraint>, Type)> {
        let t = self.btype()?;
        if self.eat(&Tok::FatArrow) {
            let ctx = self.constraints_of(&t)?;
            let head = self.btype()?;
            Ok((ctx, head))
        } else {
            Ok((Vec::new(), t))
        }
    }

    fn constraints_of(&mut self, t: &Type) -> PResult<Vec<Constraint>> {
        let parts: Vec<&Type> = match &t.kind {
            TypeKind::Tuple(ts) => ts.iter().collect(),
            TypeKind::Con(c) if c.name == "()" => Vec::new(),
            _ => vec![t],
        };
        let mut out = Vec::new();
        for p in parts {
            let (h, args) = flatten_tyapp(p);
            match &h.kind {
                TypeKind::Con(c) => out.push(Constraint {
                    class: c.clone(),
                    types: args.into_iter().cloned().collect(),
                }),
                _ => {
                    self.error_at(
                        p.span,
                        "invalid class constraint".into(),
                        "expected `Class type`".into(),
                    );
                    return Err(());
                }
            }
        }
        Ok(out)
    }

    /// A declaration in a `let`, `where`, class or instance block; or a
    /// top-level value declaration.
    fn decl(&mut self) -> PResult<Vec<Decl>> {
        let start = self.span();
        // Fixity declaration.
        let assoc = match self.cur() {
            Tok::Kw(Kw::Infixl) => Some(Assoc::Left),
            Tok::Kw(Kw::Infixr) => Some(Assoc::Right),
            Tok::Kw(Kw::Infix) => Some(Assoc::None),
            _ => None,
        };
        if let Some(assoc) = assoc {
            self.bump();
            let prec = match self.cur() {
                Tok::Int(n) if n <= 9 => {
                    self.bump();
                    n as u8
                }
                Tok::Int(_) => {
                    let sp = self.span();
                    self.error_at(
                        sp,
                        "precedence must be between 0 and 9".into(),
                        "out of range".into(),
                    );
                    self.bump();
                    9
                }
                _ => 9,
            };
            let mut ops = Vec::new();
            loop {
                match self.qop() {
                    Some((q, n)) if q.qual.is_none() => {
                        self.bump_n(n);
                        ops.push(Ident {
                            name: q.name,
                            span: q.span,
                        });
                    }
                    _ => return self.expected("an operator"),
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            return Ok(vec![Decl::Fixity(
                assoc,
                prec,
                ops,
                start.to(self.prev_span()),
            )]);
        }
        // Type signature: `v1, v2 :: type`.
        if let Some(names) = self.attempt(|p| {
            let mut names = vec![p.var_or_parens_op(false)?];
            while p.eat(&Tok::Comma) {
                names.push(p.var_or_parens_op(false)?);
            }
            if p.at(&Tok::DColon) {
                p.bump();
                Ok(names)
            } else {
                Err(())
            }
        }) {
            let ty = self.sig_type()?;
            return Ok(vec![Decl::TypeSig(names, ty, start.to(self.prev_span()))]);
        }
        // Function clause or pattern binding.
        let lhs = self.pat_seq_operands()?;
        let rhs = self.rhs(&Tok::Equals)?;
        let span = start.to(self.prev_span());
        self.classify_lhs(lhs, rhs, span)
            .map(|d| vec![self.strict_decl(d)])
    }

    /// A left-hand side: operands are runs of atomic patterns.
    fn pat_seq_operands(&mut self) -> PResult<Vec<OpItem<Vec<Pat>>>> {
        let mut items = Vec::new();
        loop {
            if self.at_varsym("-") {
                let sp = self.span();
                self.bump();
                items.push(OpItem::Neg(sp));
            }
            let mut run = Vec::new();
            while self.at_apat_start() {
                run.push(self.apat()?);
            }
            if run.is_empty() {
                return self.expected("a pattern");
            }
            items.push(OpItem::Operand(run));
            match self.qop() {
                Some((op, n)) if !(op.name == "!" && self.is_bang()) => {
                    self.bump_n(n);
                    items.push(OpItem::Op(op));
                }
                _ => break,
            }
        }
        Ok(items)
    }

    fn classify_lhs(
        &mut self,
        mut items: Vec<OpItem<Vec<Pat>>>,
        rhs: Rhs,
        span: Span,
    ) -> PResult<Decl> {
        // `x op y = ...` with a variable operator defines `op`.
        if items.len() == 3
            && let OpItem::Op(op) = &items[1]
            && !op.is_con()
            && op.qual.is_none()
        {
            let op = Ident {
                name: op.name.clone(),
                span: op.span,
            };
            let OpItem::Operand(r) = items.pop().unwrap() else {
                unreachable!()
            };
            items.pop();
            let OpItem::Operand(l) = items.pop().unwrap() else {
                unreachable!()
            };
            let l = self.run_to_pat(l)?;
            let r = self.run_to_pat(r)?;
            return Ok(Decl::Fun(op, vec![l, r], rhs, span));
        }
        if items.len() == 1 {
            let OpItem::Operand(run) = items.pop().unwrap() else {
                unreachable!()
            };
            let head = &run[0];
            match &head.kind {
                PatKind::Var(f) => {
                    let f = f.clone();
                    return Ok(Decl::Fun(f, run[1..].to_vec(), rhs, span));
                }
                // `(x `op` y) z = ...`, `(f x) y = ...`.
                PatKind::Paren(inner) if run.len() > 1 => {
                    if let Some((f, mut args)) = paren_funlhs(inner) {
                        args.extend(run[1..].iter().cloned());
                        return Ok(Decl::Fun(f, args, rhs, span));
                    }
                }
                _ => {}
            }
            let p = self.run_to_pat(run)?;
            return Ok(Decl::PatBind(p, rhs, span));
        }
        let p = self.items_to_pat(items, span)?;
        Ok(Decl::PatBind(p, rhs, span))
    }

    fn items_to_pat(&mut self, items: Vec<OpItem<Vec<Pat>>>, span: Span) -> PResult<Pat> {
        let mut out = Vec::new();
        for it in items {
            out.push(match it {
                OpItem::Operand(run) => OpItem::Operand(self.run_to_pat(run)?),
                // Variable operators are rejected by the renamer, except in
                // a parenthesized function head `(f . g) x = ...`.
                OpItem::Op(op) => OpItem::Op(op),
                OpItem::Neg(s) => OpItem::Neg(s),
            });
        }
        if out.len() == 1
            && let Some(OpItem::Operand(p)) = out.pop()
        {
            return Ok(p);
        }
        Ok(Pat {
            kind: PatKind::OpSeq(out),
            span,
        })
    }

    /// `C p1 .. pn` or a single atomic pattern.
    fn run_to_pat(&mut self, mut run: Vec<Pat>) -> PResult<Pat> {
        if run.len() == 1 {
            return Ok(run.pop().unwrap());
        }
        let span = run[0].span.to(run.last().unwrap().span);
        let head = run.remove(0);
        match head.kind {
            PatKind::Con(c, args) if args.is_empty() => Ok(Pat {
                kind: PatKind::Con(c, run),
                span,
            }),
            _ => {
                self.error_at(
                    head.span,
                    "invalid pattern".into(),
                    "only a constructor can be applied to patterns".into(),
                );
                Err(())
            }
        }
    }

    fn rhs(&mut self, eq: &Tok) -> PResult<Rhs> {
        let body = if self.at(&Tok::Bar) {
            let mut gs = Vec::new();
            while self.at(&Tok::Bar) {
                gs.push(self.guarded(eq)?);
            }
            RhsBody::Guarded(gs)
        } else {
            self.expect(eq)?;
            RhsBody::Plain(self.expr()?)
        };
        let wheres = if self.at_kw(Kw::Where) {
            self.bump();
            self.local_decls_block()
        } else {
            Vec::new()
        };
        Ok(Rhs { body, wheres })
    }

    fn guarded(&mut self, eq: &Tok) -> PResult<GuardedExpr> {
        let start = self.span();
        self.expect(&Tok::Bar)?;
        let mut guards = vec![self.qual()?];
        while self.eat(&Tok::Comma) {
            guards.push(self.qual()?);
        }
        self.expect(eq)?;
        let body = self.expr()?;
        Ok(GuardedExpr {
            guards,
            body,
            span: start.to(self.prev_span()),
        })
    }

    /// A qualifier: `p <- e`, `let decls` or a boolean expression.
    fn qual(&mut self) -> PResult<Stmt> {
        let start = self.span();
        if self.at_kw(Kw::Let) {
            // `let decls` without `in` (an expression `let .. in` is tried
            // first).
            if let Some(e) = self.attempt(|p| {
                let e = p.expr()?;
                if matches!(e.kind, ExprKind::Let(..)) {
                    Ok(e)
                } else {
                    Err(())
                }
            }) {
                return Ok(Stmt::Expr(e));
            }
            self.bump();
            let ds = self.local_decls_block();
            return Ok(Stmt::Let(ds, start.to(self.prev_span())));
        }
        if let Some(p) = self.attempt(|p| {
            let pat = p.pat()?;
            if p.at(&Tok::LArrow) {
                p.bump();
                Ok(pat)
            } else {
                Err(())
            }
        }) {
            let e = self.expr()?;
            return Ok(Stmt::Bind(p, e, start.to(self.prev_span())));
        }
        Ok(Stmt::Expr(self.expr()?))
    }

    // ---- types ----

    /// A signature type: `[forall vs.] [context =>] type`.
    fn sig_type(&mut self) -> PResult<Type> {
        let start = self.span();
        if matches!(self.cur(), Tok::VarId(None, ref s) if s == "forall") {
            self.bump();
            let vs = self.tyvars();
            if !self.at_varsym(".") {
                return self.expected("`.`");
            }
            self.bump();
            let t = self.sig_type()?;
            return Ok(Type {
                kind: TypeKind::Forall(vs, Box::new(t)),
                span: start.to(self.prev_span()),
            });
        }
        let t = self.ty()?;
        if self.eat(&Tok::FatArrow) {
            let ctx = self.constraints_of(&t)?;
            let body = self.ty()?;
            return Ok(Type {
                kind: TypeKind::Qual(ctx, Box::new(body)),
                span: start.to(self.prev_span()),
            });
        }
        Ok(t)
    }

    pub fn ty(&mut self) -> PResult<Type> {
        let start = self.span();
        if matches!(self.cur(), Tok::VarId(None, ref s) if s == "forall") {
            return self.sig_type();
        }
        let t = self.btype()?;
        if self.eat(&Tok::RArrow) {
            let r = self.ty()?;
            return Ok(Type {
                kind: TypeKind::Fun(Box::new(t), Box::new(r)),
                span: start.to(self.prev_span()),
            });
        }
        Ok(t)
    }

    fn btype(&mut self) -> PResult<Type> {
        let mut t = self.atype()?;
        while matches!(
            self.cur(),
            Tok::VarId(None, _) | Tok::ConId(..) | Tok::LParen | Tok::LBracket
        ) && !matches!(self.cur(), Tok::VarId(None, ref s) if s == "forall")
        {
            let a = self.atype()?;
            let span = t.span.to(a.span);
            t = Type {
                kind: TypeKind::App(Box::new(t), Box::new(a)),
                span,
            };
        }
        Ok(t)
    }

    fn atype(&mut self) -> PResult<Type> {
        let start = self.span();
        let kind = match self.cur() {
            Tok::VarId(None, v) => {
                self.bump();
                TypeKind::Var(v)
            }
            Tok::ConId(q, n) => {
                self.bump();
                TypeKind::Con(QName {
                    qual: q,
                    name: n,
                    span: start,
                })
            }
            Tok::LBracket => {
                self.bump();
                if self.eat(&Tok::RBracket) {
                    TypeKind::Con(QName::unqual("[]", start.to(self.prev_span())))
                } else {
                    let t = self.ty()?;
                    self.expect(&Tok::RBracket)?;
                    TypeKind::List(Box::new(t))
                }
            }
            Tok::LParen => {
                self.bump();
                if self.eat(&Tok::RParen) {
                    TypeKind::Con(QName::unqual("()", start.to(self.prev_span())))
                } else if self.at(&Tok::RArrow) && self.peek_real(1) == &Tok::RParen {
                    self.bump();
                    self.bump();
                    TypeKind::Con(QName::unqual("->", start.to(self.prev_span())))
                } else if self.at(&Tok::Comma) {
                    let mut n = 1;
                    while self.eat(&Tok::Comma) {
                        n += 1;
                    }
                    self.expect(&Tok::RParen)?;
                    TypeKind::Con(QName::unqual(
                        format!("({})", ",".repeat(n - 1)),
                        start.to(self.prev_span()),
                    ))
                } else {
                    let t = self.sig_type()?;
                    if self.eat(&Tok::DColon) {
                        let k = self.kind_sig()?;
                        self.expect(&Tok::RParen)?;
                        return Ok(Type {
                            kind: TypeKind::Kinded(Box::new(t), k),
                            span: start.to(self.prev_span()),
                        });
                    }
                    if self.at(&Tok::Comma) {
                        let mut ts = vec![t];
                        while self.eat(&Tok::Comma) {
                            ts.push(self.sig_type()?);
                        }
                        self.expect(&Tok::RParen)?;
                        TypeKind::Tuple(ts)
                    } else {
                        self.expect(&Tok::RParen)?;
                        return Ok(Type {
                            kind: t.kind,
                            span: start.to(self.prev_span()),
                        });
                    }
                }
            }
            _ => return self.expected("a type"),
        };
        Ok(Type {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    // ---- patterns ----

    /// `!` directly before a pattern with space before it is a bang pattern.
    fn is_bang(&self) -> bool {
        let t = self.real();
        let next = &self.toks[(self.pos + 1).min(self.toks.len() - 1)];
        let tight_after = next.span.start == t.span.end;
        let spaced_before = self.pos == 0 || self.toks[self.pos - 1].span.end < t.span.start;
        tight_after && spaced_before
    }

    fn at_apat_start(&self) -> bool {
        match self.cur() {
            Tok::VarId(None, _)
            | Tok::ConId(..)
            | Tok::Underscore
            | Tok::Int(_)
            | Tok::Float(_)
            | Tok::Char(_)
            | Tok::Str(_)
            | Tok::LParen
            | Tok::LBracket
            | Tok::Tilde => true,
            Tok::VarSym(None, ref s) if s == "!" => self.is_bang(),
            _ => false,
        }
    }

    pub fn pat(&mut self) -> PResult<Pat> {
        let start = self.span();
        let items = self.pat_seq_operands()?;
        let span = start.to(self.prev_span());
        let p = self.items_to_pat(items, span)?;
        if self.at(&Tok::DColon) {
            self.bump();
            let t = self.ty()?;
            return Ok(Pat {
                span: start.to(self.prev_span()),
                kind: PatKind::Typed(Box::new(p), t),
            });
        }
        Ok(p)
    }

    fn apat(&mut self) -> PResult<Pat> {
        let start = self.span();
        let kind = match self.cur() {
            Tok::VarId(None, n) => {
                self.bump();
                let id = Ident {
                    name: n,
                    span: start,
                };
                if self.real().tok == Tok::At && !matches!(self.cur(), Tok::VSemi | Tok::VRBrace) {
                    self.bump();
                    let p = self.apat()?;
                    PatKind::As(id, Box::new(p))
                } else {
                    PatKind::Var(id)
                }
            }
            Tok::ConId(q, n) => {
                self.bump();
                let c = QName {
                    qual: q,
                    name: n,
                    span: start,
                };
                if self.real().tok == Tok::LBrace
                    && !matches!(self.cur(), Tok::VSemi | Tok::VRBrace)
                {
                    self.bump();
                    let mut fs = Vec::new();
                    while !self.eat(&Tok::RBrace) {
                        let f = self.qvar()?;
                        let p = if self.eat(&Tok::Equals) {
                            Some(self.pat()?)
                        } else {
                            None
                        };
                        fs.push((f, p));
                        if !self.eat(&Tok::Comma) {
                            self.expect(&Tok::RBrace)?;
                            break;
                        }
                    }
                    PatKind::Record(c, fs)
                } else {
                    PatKind::Con(c, Vec::new())
                }
            }
            Tok::Underscore => {
                self.bump();
                PatKind::Wild
            }
            Tok::Int(n) => {
                self.bump();
                PatKind::Lit(Lit::Int(n))
            }
            Tok::Float(x) => {
                self.bump();
                PatKind::Lit(Lit::Frac(x))
            }
            Tok::Char(c) => {
                self.bump();
                PatKind::Lit(Lit::Char(c))
            }
            Tok::Str(s) => {
                self.bump();
                PatKind::Lit(Lit::Str(s))
            }
            Tok::Tilde => {
                self.bump();
                PatKind::Lazy(Box::new(self.apat()?))
            }
            Tok::VarSym(None, ref s) if s == "!" => {
                self.bump();
                PatKind::Bang(Box::new(self.apat()?))
            }
            Tok::LBracket => {
                self.bump();
                let mut ps = Vec::new();
                while !self.eat(&Tok::RBracket) {
                    ps.push(self.pat()?);
                    if !self.eat(&Tok::Comma) {
                        self.expect(&Tok::RBracket)?;
                        break;
                    }
                }
                PatKind::List(ps)
            }
            Tok::LParen => {
                self.bump();
                if self.eat(&Tok::RParen) {
                    PatKind::Con(QName::unqual("()", start.to(self.prev_span())), Vec::new())
                } else if let Tok::ConSym(q, s) = self.cur()
                    && self.peek_real(1) == &Tok::RParen
                {
                    self.bump();
                    self.bump();
                    PatKind::Con(
                        QName {
                            qual: q,
                            name: s,
                            span: start.to(self.prev_span()),
                        },
                        Vec::new(),
                    )
                } else if let Tok::VarSym(None, s) = self.cur()
                    && self.peek_real(1) == &Tok::RParen
                {
                    // `(op)` as a variable: `(>>=) = primBindIO`.
                    self.bump();
                    self.bump();
                    PatKind::Var(Ident {
                        name: s,
                        span: start.to(self.prev_span()),
                    })
                } else if self.at(&Tok::Comma) {
                    let mut n = 1;
                    while self.eat(&Tok::Comma) {
                        n += 1;
                    }
                    self.expect(&Tok::RParen)?;
                    PatKind::Con(
                        QName::unqual(
                            format!("({})", ",".repeat(n - 1)),
                            start.to(self.prev_span()),
                        ),
                        Vec::new(),
                    )
                } else if self.at_varsym("-")
                    && matches!(self.peek_real(1), Tok::Int(_) | Tok::Float(_))
                {
                    self.bump();
                    let lit = match self.cur() {
                        Tok::Int(n) => Lit::Int(n),
                        Tok::Float(x) => Lit::Frac(x),
                        _ => unreachable!(),
                    };
                    self.bump();
                    let inner = Pat {
                        kind: PatKind::NegLit(lit),
                        span: start.to(self.prev_span()),
                    };
                    if self.eat(&Tok::RParen) {
                        PatKind::Paren(Box::new(inner))
                    } else {
                        return self.expected("`)`");
                    }
                } else {
                    let p = self.pat()?;
                    if self.at(&Tok::Comma) {
                        let mut ps = vec![p];
                        while self.eat(&Tok::Comma) {
                            ps.push(self.pat()?);
                        }
                        self.expect(&Tok::RParen)?;
                        PatKind::Tuple(ps)
                    } else {
                        self.expect(&Tok::RParen)?;
                        PatKind::Paren(Box::new(p))
                    }
                }
            }
            _ => return self.expected("a pattern"),
        };
        Ok(Pat {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn qvar(&mut self) -> PResult<QName> {
        match self.cur() {
            Tok::VarId(q, n) => {
                let span = self.span();
                self.bump();
                Ok(QName {
                    qual: q,
                    name: n,
                    span,
                })
            }
            _ => self.expected("a field name"),
        }
    }

    // ---- expressions ----

    pub fn expr(&mut self) -> PResult<Expr> {
        let start = self.span();
        let e = self.infix_expr(false)?;
        if self.at(&Tok::DColon) {
            self.bump();
            let t = self.sig_type()?;
            return Ok(Expr {
                kind: ExprKind::Typed(Box::new(e), t),
                span: start.to(self.prev_span()),
            });
        }
        Ok(e)
    }

    /// An operator sequence. With `section`, a trailing operator before
    /// `)` is left for the caller (left section).
    fn infix_expr(&mut self, section: bool) -> PResult<Expr> {
        let start = self.span();
        let mut items = Vec::new();
        loop {
            if self.at_varsym("-") {
                let sp = self.span();
                self.bump();
                items.push(OpItem::Neg(sp));
                continue;
            }
            let e = self.exp10()?;
            let block_like = matches!(
                e.kind,
                ExprKind::Lam(..) | ExprKind::Let(..) | ExprKind::If(..)
            );
            items.push(OpItem::Operand(e));
            if block_like {
                break;
            }
            match self.qop() {
                Some((op, n)) => {
                    if section && self.peek_real(n) == &Tok::RParen {
                        break;
                    }
                    self.bump_n(n);
                    items.push(OpItem::Op(op));
                }
                None => break,
            }
        }
        if items.len() == 1
            && let Some(OpItem::Operand(e)) = items.pop()
        {
            return Ok(e);
        }
        Ok(Expr {
            kind: ExprKind::OpSeq(items),
            span: start.to(self.prev_span()),
        })
    }

    fn exp10(&mut self) -> PResult<Expr> {
        let start = self.span();
        let kind = match self.cur() {
            Tok::Backslash => {
                self.bump();
                if self.at_kw(Kw::Case) {
                    self.bump();
                    let alts = self.block(|p| p.alt());
                    ExprKind::LamCase(alts)
                } else {
                    let mut ps = Vec::new();
                    while self.at_apat_start() {
                        ps.push(self.apat()?);
                    }
                    if ps.is_empty() {
                        return self.expected("a pattern");
                    }
                    self.expect(&Tok::RArrow)?;
                    let body = self.expr()?;
                    let ps = ps.into_iter().map(|p| self.strict_pat(p)).collect();
                    ExprKind::Lam(ps, Box::new(body))
                }
            }
            Tok::Kw(Kw::Let) => {
                self.bump();
                let ds = self.local_decls_block();
                self.expect(&Tok::Kw(Kw::In))?;
                let body = self.expr()?;
                ExprKind::Let(ds, Box::new(body))
            }
            Tok::Kw(Kw::If) => {
                self.bump();
                if self.at(&Tok::Bar) {
                    let mut gs = Vec::new();
                    while self.at(&Tok::Bar) {
                        gs.push(self.guarded(&Tok::RArrow)?);
                    }
                    ExprKind::MultiIf(gs)
                } else {
                    let c = self.expr()?;
                    // DoAndIfThenElse: `then` / `else` may start a line of a
                    // `do` block.
                    if self.at(&Tok::VSemi) {
                        self.bump();
                    }
                    self.expect(&Tok::Kw(Kw::Then))?;
                    let t = self.expr()?;
                    if self.at(&Tok::VSemi) {
                        self.bump();
                    }
                    self.expect(&Tok::Kw(Kw::Else))?;
                    let f = self.expr()?;
                    ExprKind::If(Box::new(c), Box::new(t), Box::new(f))
                }
            }
            Tok::Kw(Kw::Case) => {
                self.bump();
                let s = self.expr()?;
                self.expect(&Tok::Kw(Kw::Of))?;
                let alts = self.block(|p| p.alt());
                ExprKind::Case(Box::new(s), alts)
            }
            Tok::Kw(Kw::Do) => {
                self.bump();
                let stmts = self.block(|p| p.qual());
                if stmts.is_empty() {
                    let sp = start.to(self.prev_span());
                    self.error_at(sp, "empty `do` block".into(), "expected statements".into());
                }
                ExprKind::Do(stmts)
            }
            _ => return self.fexp(),
        };
        Ok(Expr {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn alt(&mut self) -> PResult<Alt> {
        let start = self.span();
        let pat = self.pat()?;
        let pat = self.strict_pat(pat);
        let rhs = self.rhs(&Tok::RArrow)?;
        Ok(Alt {
            pat,
            rhs,
            span: start.to(self.prev_span()),
        })
    }

    fn at_aexp_start(&self) -> bool {
        matches!(
            self.cur(),
            Tok::VarId(..)
                | Tok::ConId(..)
                | Tok::Int(_)
                | Tok::Float(_)
                | Tok::Char(_)
                | Tok::Str(_)
                | Tok::LParen
                | Tok::LBracket
        )
    }

    fn fexp(&mut self) -> PResult<Expr> {
        let mut f = self.aexp()?;
        loop {
            if self.at_aexp_start() {
                let a = self.aexp()?;
                let span = f.span.to(a.span);
                f = Expr {
                    kind: ExprKind::App(Box::new(f), Box::new(a)),
                    span,
                };
            } else if matches!(
                self.cur(),
                Tok::Backslash
                    | Tok::Kw(Kw::Do)
                    | Tok::Kw(Kw::Case)
                    | Tok::Kw(Kw::Let)
                    | Tok::Kw(Kw::If)
            ) {
                // BlockArguments-style trailing argument: `f $ do ..` is the
                // usual form; `f do ..` is accepted as an application too.
                let a = self.exp10()?;
                let span = f.span.to(a.span);
                f = Expr {
                    kind: ExprKind::App(Box::new(f), Box::new(a)),
                    span,
                };
                break;
            } else {
                break;
            }
        }
        Ok(f)
    }

    fn aexp(&mut self) -> PResult<Expr> {
        let mut e = self.aexp1()?;
        // Record construction / update binds tighter than application.
        while self.real().tok == Tok::LBrace && !matches!(self.cur(), Tok::VSemi | Tok::VRBrace) {
            let start = e.span;
            self.bump();
            let mut fs = Vec::new();
            while !self.eat(&Tok::RBrace) {
                let f = self.qvar()?;
                let v = if self.eat(&Tok::Equals) {
                    Some(self.expr()?)
                } else {
                    None
                };
                fs.push((f, v));
                if !self.eat(&Tok::Comma) {
                    self.expect(&Tok::RBrace)?;
                    break;
                }
            }
            let span = start.to(self.prev_span());
            e = match e.kind {
                ExprKind::Con(c) => Expr {
                    kind: ExprKind::RecordCon(c, fs),
                    span,
                },
                _ => {
                    let mut upd = Vec::new();
                    for (f, v) in fs {
                        match v {
                            Some(v) => upd.push((f, v)),
                            None => {
                                self.error_at(
                                    f.span,
                                    "a record update needs `field = value`".into(),
                                    "missing `= value`".into(),
                                );
                                return Err(());
                            }
                        }
                    }
                    Expr {
                        kind: ExprKind::RecordUpd(Box::new(e), upd),
                        span,
                    }
                }
            };
        }
        Ok(e)
    }

    fn aexp1(&mut self) -> PResult<Expr> {
        let start = self.span();
        let kind = match self.cur() {
            Tok::VarId(q, n) => {
                self.bump();
                ExprKind::Var(QName {
                    qual: q,
                    name: n,
                    span: start,
                })
            }
            Tok::ConId(q, n) => {
                self.bump();
                ExprKind::Con(QName {
                    qual: q,
                    name: n,
                    span: start,
                })
            }
            Tok::Int(n) => {
                self.bump();
                ExprKind::Lit(Lit::Int(n))
            }
            Tok::Float(x) => {
                self.bump();
                ExprKind::Lit(Lit::Frac(x))
            }
            Tok::Char(c) => {
                self.bump();
                ExprKind::Lit(Lit::Char(c))
            }
            Tok::Str(s) => {
                self.bump();
                ExprKind::Lit(Lit::Str(s))
            }
            Tok::LParen => return self.paren_expr(),
            Tok::LBracket => return self.bracket_expr(),
            _ => return self.expected("an expression"),
        };
        Ok(Expr {
            kind,
            span: start.to(self.prev_span()),
        })
    }

    fn paren_expr(&mut self) -> PResult<Expr> {
        let start = self.span();
        self.bump();
        let done = |p: &Parser, kind| Expr {
            kind,
            span: start.to(p.prev_span()),
        };
        if self.eat(&Tok::RParen) {
            return Ok(done(self, ExprKind::Con(QName::unqual("()", start))));
        }
        // `(,)`, `(,,)` and tuple sections starting with a comma.
        if self.at(&Tok::Comma) {
            let mut n = 1;
            let save = self.save();
            while self.eat(&Tok::Comma) {
                n += 1;
            }
            if self.eat(&Tok::RParen) {
                let name = format!("({})", ",".repeat(n - 1));
                return Ok(done(self, ExprKind::Con(QName::unqual(name, start))));
            }
            self.restore(save);
            return self.tuple_section(start, Vec::new());
        }
        // `(op)` and right sections `(op e)`; `(- e)` is negation.
        if let Some((op, n)) = self.qop()
            && !(op.name == "-" && op.qual.is_none())
        {
            self.bump_n(n);
            if self.eat(&Tok::RParen) {
                let kind = if op.is_con() {
                    ExprKind::Con(op)
                } else {
                    ExprKind::Var(op)
                };
                return Ok(done(self, kind));
            }
            let e = self.infix_expr(false)?;
            self.expect(&Tok::RParen)?;
            return Ok(done(self, ExprKind::RightSection(op, Box::new(e))));
        }
        if self.at_varsym("-") && self.peek_real(1) == &Tok::RParen {
            self.bump();
            self.bump();
            return Ok(done(
                self,
                ExprKind::Var(QName::unqual("-", start.to(self.prev_span()))),
            ));
        }
        let e = self.infix_expr(true)?;
        // Left section `(e op)`.
        if let Some((op, n)) = self.qop()
            && self.peek_real(n) == &Tok::RParen
        {
            self.bump_n(n);
            self.bump();
            return Ok(done(self, ExprKind::LeftSection(Box::new(e), op)));
        }
        let e = if self.at(&Tok::DColon) {
            self.bump();
            let t = self.sig_type()?;
            let span = e.span.to(self.prev_span());
            Expr {
                kind: ExprKind::Typed(Box::new(e), t),
                span,
            }
        } else {
            e
        };
        if self.at(&Tok::Comma) {
            return self.tuple_section(start, vec![Some(e)]);
        }
        self.expect(&Tok::RParen)?;
        Ok(done(self, ExprKind::Paren(Box::new(e))))
    }

    /// The rest of a tuple (possibly with missing components).
    fn tuple_section(&mut self, start: Span, mut parts: Vec<Option<Expr>>) -> PResult<Expr> {
        if parts.is_empty() {
            parts.push(None);
        }
        while self.eat(&Tok::Comma) {
            if matches!(self.cur(), Tok::Comma | Tok::RParen) {
                parts.push(None);
            } else {
                parts.push(Some(self.expr()?));
            }
        }
        self.expect(&Tok::RParen)?;
        let span = start.to(self.prev_span());
        if parts.iter().all(Option::is_some) {
            return Ok(Expr {
                kind: ExprKind::Tuple(parts.into_iter().map(Option::unwrap).collect()),
                span,
            });
        }
        Ok(Expr {
            kind: ExprKind::TupleSection(parts),
            span,
        })
    }

    fn bracket_expr(&mut self) -> PResult<Expr> {
        let start = self.span();
        self.bump();
        let done = |p: &Parser, kind| Expr {
            kind,
            span: start.to(p.prev_span()),
        };
        if self.eat(&Tok::RBracket) {
            return Ok(done(self, ExprKind::Con(QName::unqual("[]", start))));
        }
        let first = self.expr()?;
        if self.eat(&Tok::DotDot) {
            let to = if self.at(&Tok::RBracket) {
                None
            } else {
                Some(Box::new(self.expr()?))
            };
            self.expect(&Tok::RBracket)?;
            return Ok(done(self, ExprKind::Range(Box::new(first), None, to)));
        }
        if self.eat(&Tok::Bar) {
            let mut qs = vec![self.qual()?];
            while self.eat(&Tok::Comma) {
                qs.push(self.qual()?);
            }
            self.expect(&Tok::RBracket)?;
            return Ok(done(self, ExprKind::ListComp(Box::new(first), qs)));
        }
        let mut es = vec![first];
        if self.eat(&Tok::Comma) {
            let second = self.expr()?;
            if self.eat(&Tok::DotDot) {
                let to = if self.at(&Tok::RBracket) {
                    None
                } else {
                    Some(Box::new(self.expr()?))
                };
                self.expect(&Tok::RBracket)?;
                let first = es.pop().unwrap();
                return Ok(done(
                    self,
                    ExprKind::Range(Box::new(first), Some(Box::new(second)), to),
                ));
            }
            es.push(second);
            while self.eat(&Tok::Comma) {
                es.push(self.expr()?);
            }
        }
        self.expect(&Tok::RBracket)?;
        Ok(done(self, ExprKind::List(es)))
    }
}

/// Tokens that cannot begin a block item: seeing one closes an implicit
/// block (parse-error(t)).
fn can_start_item(t: &Tok) -> bool {
    !matches!(
        t,
        Tok::Kw(Kw::In | Kw::Then | Kw::Else | Kw::Of | Kw::Where | Kw::Deriving)
            | Tok::RParen
            | Tok::RBracket
            | Tok::RBrace
            | Tok::Comma
            | Tok::Equals
            | Tok::RArrow
            | Tok::LArrow
            | Tok::Bar
            | Tok::DColon
            | Tok::FatArrow
            | Tok::DotDot
            | Tok::Eof
    )
}

enum TopItem {
    Import(Import),
    Decl(Vec<Decl>),
}

/// `T a b` as head and arguments.
/// A type variable binder of a declaration head: `v` or `(v :: k)`.
fn tyvar_binder(t: &Type) -> Option<(Ident, Option<KindSig>)> {
    match &t.kind {
        TypeKind::Var(v) => Some((
            Ident {
                name: v.clone(),
                span: t.span,
            },
            None,
        )),
        TypeKind::Kinded(inner, k) => {
            let (v, _) = tyvar_binder(inner)?;
            Some((v, Some(k.clone())))
        }
        _ => None,
    }
}

pub fn flatten_tyapp(t: &Type) -> (&Type, Vec<&Type>) {
    let mut args = Vec::new();
    let mut h = t;
    while let TypeKind::App(f, a) = &h.kind {
        args.push(&**a);
        h = f;
    }
    args.reverse();
    (h, args)
}

/// `(f p1 .. pn)` or `(p1 `op` p2)` as a function left-hand side.
fn paren_funlhs(p: &Pat) -> Option<(Ident, Vec<Pat>)> {
    match &p.kind {
        PatKind::OpSeq(items) if items.len() == 3 => {
            let (OpItem::Operand(l), OpItem::Op(op), OpItem::Operand(r)) =
                (&items[0], &items[1], &items[2])
            else {
                return None;
            };
            if op.is_con() {
                return None;
            }
            Some((
                Ident {
                    name: op.name.clone(),
                    span: op.span,
                },
                vec![l.clone(), r.clone()],
            ))
        }
        _ => None,
    }
}

impl Parser {
    pub fn finish(self) -> Vec<Diagnostic> {
        self.errors
    }
}
