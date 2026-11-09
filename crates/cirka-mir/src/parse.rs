//! Parser for the MIR text format (06-ir.md §2.1).
//!
//! ```text
//! type List { Nil = 0 {}, Cons = 1 { head: ptr, tail: ptr } }
//! extern fn cirka_print_i64(i64) -> void
//! global ones = ones_init
//! fn map(f: ptr @owned, xs: ptr @owned) -> ptr {
//! entry:
//!   switch xs { List.Nil => nil_case, List.Cons => cons_case }
//! nil_case:
//!   drop f
//!   ret xs
//! ...
//! }
//! ```

use crate::ir::*;
use cirka_diagnostics::{Diagnostic, FileId, Span};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
    At(String),
    Sym(&'static str),
    Eof,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    file: FileId,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || matches!(c, b'_' | b'$' | b'%' | b'\'' | b'#')
}

fn is_ident_char(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == b'.'
}

impl<'a> Lexer<'a> {
    fn err(&self, msg: impl Into<String>, start: usize) -> Diagnostic {
        Diagnostic::error(msg).with_code("M0001").with_label(
            Span::new(self.file, start as u32, self.pos.max(start + 1) as u32),
            "",
        )
    }

    fn tokens(mut self) -> Result<Vec<(Tok, Span)>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            // skip whitespace and comments
            loop {
                while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
                    self.pos += 1;
                }
                if self.src[self.pos..].starts_with(b"//") {
                    while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                } else {
                    break;
                }
            }
            let start = self.pos;
            let file = self.file;
            let sp = move |end: usize| Span::new(file, start as u32, end as u32);
            if self.pos >= self.src.len() {
                out.push((Tok::Eof, sp(start)));
                return Ok(out);
            }
            let c = self.src[self.pos];
            let tok = if is_ident_start(c) {
                while self.pos < self.src.len() && is_ident_char(self.src[self.pos]) {
                    self.pos += 1;
                }
                // do not swallow a trailing '.' (not used, but be strict)
                Tok::Ident(String::from_utf8_lossy(&self.src[start..self.pos]).into_owned())
            } else if c == b'@' {
                self.pos += 1;
                while self.pos < self.src.len() && is_ident_char(self.src[self.pos]) {
                    self.pos += 1;
                }
                Tok::At(String::from_utf8_lossy(&self.src[start + 1..self.pos]).into_owned())
            } else if c.is_ascii_digit()
                || (c == b'-'
                    && self
                        .src
                        .get(self.pos + 1)
                        .is_some_and(|d| d.is_ascii_digit()))
            {
                self.pos += 1;
                let mut float = false;
                while self.pos < self.src.len() {
                    let d = self.src[self.pos];
                    if d.is_ascii_digit() || d == b'_' {
                        self.pos += 1;
                    } else if (d == b'.' || d == b'e' || d == b'E')
                        && self.src.get(self.pos + 1).is_some_and(|e| {
                            e.is_ascii_digit()
                                || ((d == b'e' || d == b'E') && (*e == b'-' || *e == b'+'))
                        })
                    {
                        float = true;
                        self.pos += 2;
                    } else {
                        break;
                    }
                }
                let text: String =
                    String::from_utf8_lossy(&self.src[start..self.pos]).replace('_', "");
                if float {
                    Tok::Float(
                        text.parse()
                            .map_err(|_| self.err("bad float literal", start))?,
                    )
                } else {
                    Tok::Int(
                        text.parse()
                            .map_err(|_| self.err("bad integer literal", start))?,
                    )
                }
            } else if c == b'"' {
                self.pos += 1;
                let mut bytes = Vec::new();
                loop {
                    let Some(&d) = self.src.get(self.pos) else {
                        return Err(self.err("unterminated string", start));
                    };
                    self.pos += 1;
                    match d {
                        b'"' => break,
                        b'\\' => {
                            let Some(&e) = self.src.get(self.pos) else {
                                return Err(self.err("unterminated string", start));
                            };
                            self.pos += 1;
                            match e {
                                b'n' => bytes.push(b'\n'),
                                b't' => bytes.push(b'\t'),
                                b'r' => bytes.push(b'\r'),
                                b'0' => bytes.push(0),
                                b'\\' => bytes.push(b'\\'),
                                b'"' => bytes.push(b'"'),
                                b'x' => {
                                    let h = self
                                        .src
                                        .get(self.pos..self.pos + 2)
                                        .and_then(|h| std::str::from_utf8(h).ok())
                                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                                        .ok_or_else(|| self.err("bad \\x escape", start))?;
                                    self.pos += 2;
                                    bytes.push(h);
                                }
                                _ => return Err(self.err("unknown escape", start)),
                            }
                        }
                        _ => bytes.push(d),
                    }
                }
                Tok::Str(bytes)
            } else {
                let syms: &[&'static str] = &["=>", "->", "{", "}", "(", ")", ",", ":", "=", "."];
                let s = syms
                    .iter()
                    .find(|s| self.src[self.pos..].starts_with(s.as_bytes()))
                    .ok_or_else(|| {
                        self.pos += 1;
                        self.err(format!("unexpected character `{}`", c as char), start)
                    })?;
                self.pos += s.len();
                Tok::Sym(s)
            };
            out.push((tok, sp(self.pos)));
        }
    }
}

// ---- raw (name-based) syntax, resolved after the whole module is read ----

#[derive(Clone, Debug)]
enum ROp {
    Var(String, Span),
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
}

#[derive(Clone, Debug)]
enum RRhs {
    Lit(ROp),
    Prim(String, Vec<ROp>),
    Alloc(String, Vec<ROp>, Option<(String, Span)>),
    Proj((String, Span), String),
    Call(String, Vec<ROp>),
    Apply((String, Span), Vec<ROp>),
    MkClosure(String, Vec<ROp>),
    MkThunk(String, Vec<ROp>),
    Force((String, Span)),
    DropReuse((String, Span)),
    Extern(String, Vec<ROp>),
    Global(String),
}

#[derive(Clone, Debug)]
enum RInstr {
    Let(Option<(String, Span)>, Option<Ty>, RRhs, Span),
    /// `let (x1, .., xn) = call f(args)`.
    LetMulti(Vec<(String, Span)>, (String, Span), Vec<ROp>),
    Dup((String, Span)),
    Drop((String, Span)),
    Set((String, Span), String, ROp),
}

#[derive(Clone, Debug)]
enum RTerm {
    Ret(ROp),
    RetMulti(Vec<ROp>),
    Jump((String, Span), Vec<ROp>),
    Switch(
        (String, Span),
        Vec<(Option<String>, Option<i64>, (String, Span))>,
    ),
    TailCall(String, Vec<ROp>),
    Panic(String),
    Unreachable,
}

struct RBlock {
    label: String,
    span: Span,
    params: Vec<(String, Ty, Span)>,
    instrs: Vec<RInstr>,
    term: RTerm,
}

struct RFunc {
    cold: bool,
    name: String,
    span: Span,
    params: Vec<(String, Ty, Own, Span)>,
    /// One result type, or n >= 2 for `-> (t1, .., tn)`.
    ret: Vec<Ty>,
    blocks: Vec<RBlock>,
}

struct Parser {
    toks: Vec<(Tok, Span)>,
    pos: usize,
}

type PResult<T> = Result<T, Diagnostic>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }
    fn span(&self) -> Span {
        self.toks[self.pos].1
    }
    fn bump(&mut self) -> (Tok, Span) {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(Diagnostic::error(msg)
            .with_code("M0002")
            .with_label(self.span(), ""))
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }
    fn is_kw(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Ident(x) if x == s)
    }
    fn expect_sym(&mut self, s: &str) -> PResult<()> {
        if self.is_sym(s) {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected `{s}`, found {}", describe(self.peek())))
        }
    }
    fn expect_kw(&mut self, s: &str) -> PResult<()> {
        if self.is_kw(s) {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected `{s}`, found {}", describe(self.peek())))
        }
    }
    fn ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                let sp = self.span();
                self.bump();
                Ok((s, sp))
            }
            t => self.err(format!("expected identifier, found {}", describe(&t))),
        }
    }
    fn ty(&mut self) -> PResult<Ty> {
        let (s, _) = self.ident()?;
        match Ty::from_name(&s) {
            Some(t) => Ok(t),
            None => {
                self.pos -= 1;
                self.err(format!("unknown type `{s}`"))
            }
        }
    }
    fn c_ty(&mut self) -> PResult<cirka_core::CTy> {
        let (s, _) = self.ident()?;
        match cirka_core::CTy::from_name(&s) {
            Some(t) => Ok(t),
            None => {
                self.pos -= 1;
                self.err(format!("unknown C type `{s}`"))
            }
        }
    }
    fn own(&mut self) -> PResult<Own> {
        match self.peek().clone() {
            Tok::At(s) if s == "owned" => {
                self.bump();
                Ok(Own::Owned)
            }
            Tok::At(s) if s == "borrowed" => {
                self.bump();
                Ok(Own::Borrowed)
            }
            Tok::At(s) => self.err(format!("unknown annotation `@{s}`")),
            _ => Ok(Own::Owned),
        }
    }
    fn comma_list<T>(
        &mut self,
        open: &str,
        close: &str,
        mut f: impl FnMut(&mut Parser) -> PResult<T>,
    ) -> PResult<Vec<T>> {
        self.expect_sym(open)?;
        let mut v = Vec::new();
        while !self.is_sym(close) {
            v.push(f(self)?);
            if self.is_sym(",") {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_sym(close)?;
        Ok(v)
    }
    fn operand(&mut self) -> PResult<ROp> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                let sp = self.span();
                self.bump();
                Ok(ROp::Var(s, sp))
            }
            Tok::Int(i) => {
                self.bump();
                Ok(ROp::Int(i))
            }
            Tok::Float(f) => {
                self.bump();
                Ok(ROp::Float(f))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(ROp::Str(s))
            }
            t => self.err(format!("expected operand, found {}", describe(&t))),
        }
    }
    fn args(&mut self) -> PResult<Vec<ROp>> {
        self.comma_list("(", ")", |p| p.operand())
    }

    fn module(&mut self, m: &mut Module, funcs: &mut Vec<RFunc>) -> PResult<()> {
        loop {
            match self.peek().clone() {
                Tok::Eof => return Ok(()),
                Tok::Ident(k) if k == "type" => self.typedef(m)?,
                Tok::Ident(k) if k == "extern" => self.externdecl(m)?,
                Tok::Ident(k) if k == "global" => {
                    self.bump();
                    let (name, _) = self.ident()?;
                    self.expect_sym("=")?;
                    let (init, _) = self.ident()?;
                    m.globals.push(GlobalDecl { name, init });
                }
                Tok::Ident(k) if k == "fn" || k == "cold" => funcs.push(self.function()?),
                t => {
                    return self.err(format!(
                        "expected `type`, `extern`, `global` or `fn`, found {}",
                        describe(&t)
                    ));
                }
            }
        }
    }

    fn typedef(&mut self, m: &mut Module) -> PResult<()> {
        self.expect_kw("type")?;
        let (name, sp) = self.ident()?;
        if m.types.iter().any(|t| t.name == name) {
            return Err(Diagnostic::error(format!("duplicate type `{name}`")).with_label(sp, ""));
        }
        let ctors = self.comma_list("{", "}", |p| {
            let (cname, _) = p.ident()?;
            p.expect_sym("=")?;
            let tag = match p.bump().0 {
                Tok::Int(i) if (0..=0xffff).contains(&i) => i as u32,
                _ => {
                    p.pos -= 1;
                    return p.err("expected constructor tag");
                }
            };
            let fields = p.comma_list("{", "}", |p| {
                let (fname, _) = p.ident()?;
                p.expect_sym(":")?;
                let ty = p.ty()?;
                Ok(Field { name: fname, ty })
            })?;
            Ok(CtorDef {
                name: cname,
                tag,
                fields,
            })
        })?;
        m.types.push(TypeDef { name, ctors });
        Ok(())
    }

    fn externdecl(&mut self, m: &mut Module) -> PResult<()> {
        self.expect_kw("extern")?;
        self.expect_kw("fn")?;
        let (name, _) = self.ident()?;
        let params = self.comma_list("(", ")", |p| {
            let t = p.ty()?;
            let o = if p.is_sym(",") || p.is_sym(")") {
                Own::Borrowed
            } else {
                p.own()?
            };
            Ok((t, o))
        })?;
        self.expect_sym("->")?;
        let ret = if self.is_kw("void") {
            self.bump();
            None
        } else {
            Some(self.ty()?)
        };
        // `as c(i32, ptr) -> i32`: the C signature of a foreign function.
        let c_sig = if self.is_kw("as") {
            self.bump();
            self.expect_kw("c")?;
            let ps = self.comma_list("(", ")", |p| p.c_ty())?;
            self.expect_sym("->")?;
            let ret = self.c_ty()?;
            Some(cirka_core::CSig { params: ps, ret })
        } else {
            None
        };
        m.externs.push(ExternDecl {
            name,
            params,
            ret,
            c_sig,
        });
        Ok(())
    }

    fn function(&mut self) -> PResult<RFunc> {
        let cold = self.is_kw("cold");
        if cold {
            self.bump();
        }
        self.expect_kw("fn")?;
        let (name, span) = self.ident()?;
        let params = self.comma_list("(", ")", |p| {
            let (n, sp) = p.ident()?;
            p.expect_sym(":")?;
            let t = p.ty()?;
            let o = p.own()?;
            Ok((n, t, o, sp))
        })?;
        self.expect_sym("->")?;
        let ret = if self.is_sym("(") {
            let sp = self.span();
            let tys = self.comma_list("(", ")", |p| p.ty())?;
            if tys.len() < 2 {
                return Err(
                    Diagnostic::error("multiple results need at least two types")
                        .with_code("M0002")
                        .with_label(sp, ""),
                );
            }
            tys
        } else {
            vec![self.ty()?]
        };
        self.expect_sym("{")?;
        let mut blocks = Vec::new();
        while !self.is_sym("}") {
            blocks.push(self.block()?);
        }
        self.expect_sym("}")?;
        if blocks.is_empty() {
            return Err(
                Diagnostic::error(format!("function `{name}` has no blocks")).with_label(span, ""),
            );
        }
        Ok(RFunc {
            cold,
            name,
            span,
            params,
            ret,
            blocks,
        })
    }

    fn block(&mut self) -> PResult<RBlock> {
        let (label, span) = self.ident()?;
        let params = if self.is_sym("(") {
            self.comma_list("(", ")", |p| {
                let (n, sp) = p.ident()?;
                p.expect_sym(":")?;
                let t = p.ty()?;
                Ok((n, t, sp))
            })?
        } else {
            Vec::new()
        };
        self.expect_sym(":")?;
        let mut instrs = Vec::new();
        loop {
            let sp = self.span();
            let Tok::Ident(k) = self.peek().clone() else {
                return self.err(format!(
                    "expected instruction, found {}",
                    describe(self.peek())
                ));
            };
            match k.as_str() {
                "let" if matches!(self.toks.get(self.pos + 1), Some((Tok::Sym("("), _))) => {
                    self.bump();
                    let names = self.comma_list("(", ")", |p| p.ident())?;
                    self.expect_sym("=")?;
                    self.expect_kw("call")?;
                    let func = self.ident()?;
                    let args = self.args()?;
                    instrs.push(RInstr::LetMulti(names, func, args));
                }
                "let" => {
                    self.bump();
                    let (n, nsp) = self.ident()?;
                    let ty = if self.is_sym(":") {
                        self.bump();
                        Some(self.ty()?)
                    } else {
                        None
                    };
                    self.expect_sym("=")?;
                    let rhs = self.rhs()?;
                    let name = if n == "_" { None } else { Some((n, nsp)) };
                    instrs.push(RInstr::Let(name, ty, rhs, sp));
                }
                "dup" => {
                    self.bump();
                    instrs.push(RInstr::Dup(self.ident()?));
                }
                "drop" => {
                    self.bump();
                    instrs.push(RInstr::Drop(self.ident()?));
                }
                "set" => {
                    self.bump();
                    let obj = self.ident()?;
                    let (path, _) = self.ident()?;
                    self.expect_sym("=")?;
                    let v = self.operand()?;
                    instrs.push(RInstr::Set(obj, path, v));
                }
                _ => break,
            }
        }
        let term = self.term()?;
        Ok(RBlock {
            label,
            span,
            params,
            instrs,
            term,
        })
    }

    fn rhs(&mut self) -> PResult<RRhs> {
        let (k, _) = self.ident()?;
        Ok(match k.as_str() {
            "lit" => RRhs::Lit(self.operand()?),
            "prim" => {
                let (op, _) = self.ident()?;
                RRhs::Prim(op, self.args()?)
            }
            "alloc" => {
                let (c, _) = self.ident()?;
                let args = self.args()?;
                let reuse = if self.is_kw("reuse") {
                    self.bump();
                    Some(self.ident()?)
                } else {
                    None
                };
                RRhs::Alloc(c, args, reuse)
            }
            "proj" => {
                let v = self.ident()?;
                let (path, _) = self.ident()?;
                RRhs::Proj(v, path)
            }
            "call" => {
                let (f, _) = self.ident()?;
                RRhs::Call(f, self.args()?)
            }
            "apply" => {
                let c = self.ident()?;
                RRhs::Apply(c, self.args()?)
            }
            "mkclosure" => {
                let (f, _) = self.ident()?;
                RRhs::MkClosure(f, self.args()?)
            }
            "mkthunk" => {
                let (f, _) = self.ident()?;
                RRhs::MkThunk(f, self.args()?)
            }
            "force" => RRhs::Force(self.ident()?),
            "drop_reuse" => RRhs::DropReuse(self.ident()?),
            "extern" => {
                let (f, _) = self.ident()?;
                RRhs::Extern(f, self.args()?)
            }
            "global" => RRhs::Global(self.ident()?.0),
            _ => {
                self.pos -= 1;
                return self.err(format!("unknown instruction `{k}`"));
            }
        })
    }

    fn term(&mut self) -> PResult<RTerm> {
        let (k, _) = self.ident()?;
        Ok(match k.as_str() {
            "ret" if self.is_sym("(") => RTerm::RetMulti(self.args()?),
            "ret" => RTerm::Ret(self.operand()?),
            "jump" => {
                let l = self.ident()?;
                let args = if self.is_sym("(") {
                    self.args()?
                } else {
                    Vec::new()
                };
                RTerm::Jump(l, args)
            }
            "switch" => {
                let v = self.ident()?;
                let arms = self.comma_list("{", "}", |p| {
                    let (c, i) = match p.peek().clone() {
                        Tok::Int(i) => {
                            p.bump();
                            (None, Some(i))
                        }
                        Tok::Ident(s) => {
                            p.bump();
                            (if s == "default" { None } else { Some(s) }, None)
                        }
                        t => return p.err(format!("expected switch arm, found {}", describe(&t))),
                    };
                    p.expect_sym("=>")?;
                    let l = p.ident()?;
                    Ok((c, i, l))
                })?;
                RTerm::Switch(v, arms)
            }
            "tailcall" => {
                let (f, _) = self.ident()?;
                RTerm::TailCall(f, self.args()?)
            }
            "panic" => match self.bump().0 {
                Tok::Str(s) => RTerm::Panic(String::from_utf8_lossy(&s).into_owned()),
                _ => {
                    self.pos -= 1;
                    return self.err("expected panic message string");
                }
            },
            "unreachable" => RTerm::Unreachable,
            _ => {
                self.pos -= 1;
                return self.err(format!("expected instruction or terminator, found `{k}`"));
            }
        })
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("`{s}`"),
        Tok::Int(i) => format!("`{i}`"),
        Tok::Float(f) => format!("`{f}`"),
        Tok::Str(_) => "string".into(),
        Tok::At(s) => format!("`@{s}`"),
        Tok::Sym(s) => format!("`{s}`"),
        Tok::Eof => "end of file".into(),
    }
}

/// Signatures used to type operands and results while resolving.
struct Sigs<'a> {
    m: &'a Module,
    funcs: HashMap<String, (Vec<Ty>, Vec<Ty>)>,
}

impl Sigs<'_> {
    fn ctor_path(&self, path: &str, sp: Span) -> PResult<CtorRef> {
        let mut it = path.rsplitn(2, '.');
        let c = it.next().unwrap_or("");
        let t = it.next().unwrap_or("");
        self.m.find_ctor(t, c).ok_or_else(|| {
            Diagnostic::error(format!("unknown constructor `{path}`"))
                .with_code("M0003")
                .with_label(sp, "")
        })
    }
    fn field_path(&self, path: &str, sp: Span) -> PResult<(CtorRef, u32)> {
        let (cpath, field) = path.rsplit_once('.').ok_or_else(|| {
            Diagnostic::error(format!("expected `Type.Ctor.field`, found `{path}`"))
                .with_label(sp, "")
        })?;
        let c = self.ctor_path(cpath, sp)?;
        let fi = self.m.ctor(c).field_index(field).ok_or_else(|| {
            Diagnostic::error(format!("constructor `{cpath}` has no field `{field}`"))
                .with_code("M0003")
                .with_label(sp, "")
        })?;
        Ok((c, fi as u32))
    }
}

struct FnResolver<'a> {
    f: Function,
    names: HashMap<String, Var>,
    labels: HashMap<String, BlockId>,
    sigs: &'a Sigs<'a>,
}

impl FnResolver<'_> {
    fn var(&self, (n, sp): &(String, Span)) -> PResult<Var> {
        self.names.get(n).copied().ok_or_else(|| {
            Diagnostic::error(format!("undefined variable `{n}`"))
                .with_code("M0004")
                .with_label(*sp, "")
        })
    }
    fn label(&self, (n, sp): &(String, Span)) -> PResult<BlockId> {
        self.labels.get(n).copied().ok_or_else(|| {
            Diagnostic::error(format!("undefined block `{n}`"))
                .with_code("M0004")
                .with_label(*sp, "")
        })
    }
    fn op(&self, o: &ROp, expect: Option<Ty>) -> PResult<Operand> {
        Ok(match o {
            ROp::Var(n, sp) => Operand::Var(self.var(&(n.clone(), *sp))?),
            ROp::Int(i) => {
                let t = match expect {
                    Some(t @ (Ty::I32 | Ty::I8 | Ty::I64)) => t,
                    Some(Ty::F64) => return Ok(Operand::Const(Const::Float(*i as f64))),
                    _ => Ty::I64,
                };
                Operand::Const(Const::Int(*i, t))
            }
            ROp::Float(f) => Operand::Const(Const::Float(*f)),
            ROp::Str(s) => Operand::Const(Const::Str(s.clone())),
        })
    }
    fn ops(&self, os: &[ROp], expect: &[Ty]) -> PResult<Vec<Operand>> {
        os.iter()
            .enumerate()
            .map(|(i, o)| self.op(o, expect.get(i).copied()))
            .collect()
    }
    fn fn_sig(&self, name: &str) -> (Vec<Ty>, Option<Vec<Ty>>) {
        match self.sigs.funcs.get(name) {
            Some((p, r)) => (p.clone(), Some(r.clone())),
            None => (Vec::new(), None),
        }
    }
}

/// Parses a MIR module. Errors carry spans into `file`.
pub fn parse(file: FileId, src: &str) -> Result<Module, Diagnostic> {
    let toks = Lexer {
        src: src.as_bytes(),
        pos: 0,
        file,
    }
    .tokens()?;
    let mut p = Parser { toks, pos: 0 };
    let mut m = Module::default();
    let mut rfuncs = Vec::new();
    p.module(&mut m, &mut rfuncs)?;

    let mut funcs = HashMap::new();
    for rf in &rfuncs {
        if funcs.contains_key(&rf.name) {
            return Err(
                Diagnostic::error(format!("duplicate function `{}`", rf.name))
                    .with_code("M0005")
                    .with_label(rf.span, ""),
            );
        }
        funcs.insert(
            rf.name.clone(),
            (
                rf.params.iter().map(|p| p.1).collect::<Vec<_>>(),
                rf.ret.clone(),
            ),
        );
    }
    let sigs = Sigs { m: &m, funcs };
    let mut out = Vec::new();
    for rf in &rfuncs {
        out.push(resolve_fn(rf, &sigs)?);
    }
    m.funcs = out;
    Ok(m)
}

fn resolve_fn(rf: &RFunc, sigs: &Sigs<'_>) -> PResult<Function> {
    let mut r = FnResolver {
        f: if rf.ret.len() == 1 {
            Function::new(rf.name.clone(), rf.ret[0])
        } else {
            Function::new_multi(rf.name.clone(), rf.ret.clone())
        },
        names: HashMap::new(),
        labels: HashMap::new(),
        sigs,
    };
    r.f.span = rf.span;
    r.f.cold = rf.cold;
    let dup_err = |n: &str, sp: Span| {
        Diagnostic::error(format!("variable `{n}` is defined more than once"))
            .with_code("M0005")
            .with_label(sp, "")
    };
    for (n, t, o, sp) in &rf.params {
        if r.names.contains_key(n) {
            return Err(dup_err(n, *sp));
        }
        let v = r.f.new_var(n.clone(), *t);
        r.names.insert(n.clone(), v);
        r.f.params.push(Param { var: v, own: *o });
    }
    for (i, b) in rf.blocks.iter().enumerate() {
        if r.labels
            .insert(b.label.clone(), BlockId(i as u32))
            .is_some()
        {
            return Err(Diagnostic::error(format!("duplicate block `{}`", b.label))
                .with_code("M0005")
                .with_label(b.span, ""));
        }
    }
    // Define every variable first (uses may precede definitions in text order).
    for b in &rf.blocks {
        for (n, t, sp) in &b.params {
            if r.names.contains_key(n) {
                return Err(dup_err(n, *sp));
            }
            let v = r.f.new_var(n.clone(), *t);
            r.names.insert(n.clone(), v);
        }
        for ins in &b.instrs {
            if let RInstr::Let(Some((n, sp)), ann, rhs, _) = ins {
                if r.names.contains_key(n) {
                    return Err(dup_err(n, *sp));
                }
                let ty = rhs_ty(rhs, *ann, sigs, *sp)?;
                let v = r.f.new_var(n.clone(), ty);
                r.names.insert(n.clone(), v);
            }
            if let RInstr::LetMulti(names, (func, fsp), _) = ins {
                let Some((_, rets)) = sigs.funcs.get(func) else {
                    return Err(Diagnostic::error(format!("unknown function `{func}`"))
                        .with_code("M0003")
                        .with_label(*fsp, ""));
                };
                for (i, (n, sp)) in names.iter().enumerate() {
                    if r.names.contains_key(n) {
                        return Err(dup_err(n, *sp));
                    }
                    // Extra names of an arity mismatch get `ptr`: the
                    // verifier reports the mismatch.
                    let ty = rets.get(i).copied().unwrap_or(Ty::Ptr);
                    let v = r.f.new_var(n.clone(), ty);
                    r.names.insert(n.clone(), v);
                }
            }
        }
    }
    let mut blocks = Vec::new();
    for b in &rf.blocks {
        let params = b.params.iter().map(|(n, _, _)| r.names[n]).collect();
        let mut instrs = Vec::new();
        for ins in &b.instrs {
            instrs.push(match ins {
                RInstr::Let(name, ann, rhs, sp) => {
                    let var = name.as_ref().map(|n| r.names[&n.0]);
                    let rhs = resolve_rhs(&r, rhs, *ann, *sp)?;
                    Instr::Let { var, rhs }
                }
                RInstr::LetMulti(names, (func, _), args) => {
                    let (ptys, _) = r.fn_sig(func);
                    Instr::LetMulti {
                        vars: names.iter().map(|n| r.names[&n.0]).collect(),
                        func: func.clone(),
                        args: r.ops(args, &ptys)?,
                    }
                }
                RInstr::Dup(v) => Instr::Dup(r.var(v)?),
                RInstr::Drop(v) => Instr::Drop(r.var(v)?),
                RInstr::Set(o, path, val) => {
                    let (ctor, field) = sigs.field_path(path, o.1)?;
                    let fty = sigs.m.ctor(ctor).fields[field as usize].ty;
                    Instr::Set {
                        obj: r.var(o)?,
                        ctor,
                        field,
                        val: r.op(val, Some(fty))?,
                    }
                }
            });
        }
        let term = match &b.term {
            RTerm::Ret(o) => Terminator::Ret(r.op(o, Some(rf.ret[0]))?),
            RTerm::RetMulti(os) => Terminator::RetMulti(r.ops(os, &rf.ret)?),
            RTerm::Jump(l, args) => {
                let target = r.label(l)?;
                let tys: Vec<Ty> = rf.blocks[target.0 as usize]
                    .params
                    .iter()
                    .map(|p| p.1)
                    .collect();
                Terminator::Jump(target, r.ops(args, &tys)?)
            }
            RTerm::Switch(v, arms) => {
                let scrut = r.var(v)?;
                let mut out = Vec::new();
                let mut default = None;
                for (c, i, l) in arms {
                    let target = r.label(l)?;
                    match (c, i) {
                        (Some(c), _) => out.push((Case::Ctor(sigs.ctor_path(c, l.1)?), target)),
                        (None, Some(i)) => out.push((Case::Int(*i), target)),
                        (None, None) => default = Some(target),
                    }
                }
                Terminator::Switch {
                    scrut,
                    arms: out,
                    default,
                }
            }
            RTerm::TailCall(f, args) => {
                let (ptys, _) = r.fn_sig(f);
                Terminator::TailCall {
                    func: f.clone(),
                    args: r.ops(args, &ptys)?,
                }
            }
            RTerm::Panic(s) => Terminator::Panic(s.clone()),
            RTerm::Unreachable => Terminator::Unreachable,
        };
        blocks.push(Block {
            label: b.label.clone(),
            params,
            instrs,
            term,
        });
    }
    r.f.blocks = blocks;
    Ok(r.f)
}

fn rhs_ty(rhs: &RRhs, ann: Option<Ty>, sigs: &Sigs<'_>, sp: Span) -> PResult<Ty> {
    let t = match rhs {
        RRhs::Lit(o) => match (o, ann) {
            (ROp::Int(_), Some(t)) => t,
            (ROp::Int(_), None) => Ty::I64,
            (ROp::Float(_), _) => Ty::F64,
            (ROp::Str(_), _) => Ty::Ptr,
            (ROp::Var(..), _) => {
                return Err(Diagnostic::error("`lit` needs a constant").with_label(sp, ""));
            }
        },
        RRhs::Prim(op, _) => match Prim::from_name(op) {
            Some(p) => p.signature().1,
            None => {
                return Err(Diagnostic::error(format!("unknown primitive `{op}`"))
                    .with_code("M0003")
                    .with_label(sp, ""));
            }
        },
        RRhs::Proj(_, path) => {
            let (c, f) = sigs.field_path(path, sp)?;
            sigs.m.ctor(c).fields[f as usize].ty
        }
        RRhs::Call(f, _) => match sigs.funcs.get(f) {
            // The first result for a multi-result callee: the verifier
            // reports the arity mismatch.
            Some((_, r)) => r[0],
            None => {
                return Err(Diagnostic::error(format!("unknown function `{f}`"))
                    .with_code("M0003")
                    .with_label(sp, ""));
            }
        },
        RRhs::Extern(f, _) => match sigs.m.extern_decl(f) {
            Some(d) => match d.ret {
                Some(t) => t,
                None => {
                    return Err(Diagnostic::error(format!(
                        "`{f}` returns void; bind it with `let _`"
                    ))
                    .with_label(sp, ""));
                }
            },
            None => {
                return Err(Diagnostic::error(format!("undeclared extern `{f}`"))
                    .with_code("M0003")
                    .with_label(sp, ""));
            }
        },
        _ => Ty::Ptr,
    };
    if let Some(a) = ann
        && a != t
    {
        return Err(
            Diagnostic::error(format!("annotation `{a}` does not match `{t}`"))
                .with_code("M0006")
                .with_label(sp, ""),
        );
    }
    Ok(t)
}

fn resolve_rhs(r: &FnResolver<'_>, rhs: &RRhs, ann: Option<Ty>, sp: Span) -> PResult<Rhs> {
    let sigs = r.sigs;
    Ok(match rhs {
        RRhs::Lit(o) => match r.op(o, ann)? {
            Operand::Const(c) => Rhs::Lit(c),
            Operand::Var(_) => unreachable!(),
        },
        RRhs::Prim(op, args) => {
            let p = Prim::from_name(op)
                .ok_or_else(|| Diagnostic::error("unknown primitive").with_label(sp, ""))?;
            Rhs::Prim(p, r.ops(args, p.signature().0)?)
        }
        RRhs::Alloc(c, args, reuse) => {
            let ctor = sigs.ctor_path(c, sp)?;
            let def = sigs.m.ctor(ctor);
            let tys: Vec<Ty> = def.fields.iter().map(|f| f.ty).collect();
            Rhs::Alloc {
                ctor,
                args: r.ops(args, &tys)?,
                reuse: reuse.as_ref().map(|v| r.var(v)).transpose()?,
            }
        }
        RRhs::Proj(v, path) => {
            let (ctor, field) = sigs.field_path(path, v.1)?;
            Rhs::Proj {
                obj: r.var(v)?,
                ctor,
                field,
            }
        }
        RRhs::Call(f, args) => {
            let (ptys, _) = r.fn_sig(f);
            Rhs::Call {
                func: f.clone(),
                args: r.ops(args, &ptys)?,
            }
        }
        RRhs::Apply(c, args) => Rhs::Apply {
            clo: r.var(c)?,
            args: r.ops(args, &[])?,
        },
        RRhs::MkClosure(f, args) => {
            let (ptys, _) = r.fn_sig(f);
            Rhs::MkClosure {
                func: f.clone(),
                args: r.ops(args, &ptys)?,
            }
        }
        RRhs::MkThunk(f, args) => {
            let (ptys, _) = r.fn_sig(f);
            Rhs::MkThunk {
                func: f.clone(),
                args: r.ops(args, &ptys)?,
            }
        }
        RRhs::Force(v) => Rhs::Force(r.var(v)?),
        RRhs::DropReuse(v) => Rhs::DropReuse(r.var(v)?),
        RRhs::Extern(f, args) => {
            let tys: Vec<Ty> = sigs
                .m
                .extern_decl(f)
                .map(|d| d.params.iter().map(|p| p.0).collect())
                .unwrap_or_default();
            if sigs.m.extern_decl(f).is_none() {
                return Err(Diagnostic::error(format!("undeclared extern `{f}`"))
                    .with_code("M0003")
                    .with_label(sp, ""));
            }
            Rhs::Extern {
                func: f.clone(),
                args: r.ops(args, &tys)?,
            }
        }
        RRhs::Global(g) => Rhs::Global(g.clone()),
    })
}
