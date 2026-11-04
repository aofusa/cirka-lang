//! Lexical analysis of Haskell 2010 (Report §2).

use cirka_diagnostics::{Diagnostic, FileId, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// Variable identifier with an optional module qualifier (`M.N.x`).
    VarId(Option<String>, String),
    ConId(Option<String>, String),
    /// Variable operator (`+`, `M.+`). `-` and `!` are ordinary operators.
    VarSym(Option<String>, String),
    /// Constructor operator (`:`, `:+`).
    ConSym(Option<String>, String),
    Int(u128),
    Float(f64),
    Char(char),
    Str(String),
    Kw(Kw),
    // reservedop
    DotDot,
    DColon,
    Equals,
    Backslash,
    Bar,
    LArrow,
    RArrow,
    At,
    Tilde,
    FatArrow,
    // special
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Backtick,
    Underscore,
    // Inserted by the layout algorithm.
    VLBrace,
    VSemi,
    VRBrace,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Case,
    Class,
    Data,
    Default,
    Deriving,
    Do,
    Else,
    Foreign,
    If,
    Import,
    In,
    Infix,
    Infixl,
    Infixr,
    Instance,
    Let,
    Module,
    Newtype,
    Of,
    Then,
    Type,
    Where,
}

impl Kw {
    const ALL: &'static [(&'static str, Kw)] = &[
        ("case", Kw::Case),
        ("class", Kw::Class),
        ("data", Kw::Data),
        ("default", Kw::Default),
        ("deriving", Kw::Deriving),
        ("do", Kw::Do),
        ("else", Kw::Else),
        ("foreign", Kw::Foreign),
        ("if", Kw::If),
        ("import", Kw::Import),
        ("in", Kw::In),
        ("infix", Kw::Infix),
        ("infixl", Kw::Infixl),
        ("infixr", Kw::Infixr),
        ("instance", Kw::Instance),
        ("let", Kw::Let),
        ("module", Kw::Module),
        ("newtype", Kw::Newtype),
        ("of", Kw::Of),
        ("then", Kw::Then),
        ("type", Kw::Type),
        ("where", Kw::Where),
    ];

    pub fn from_name(s: &str) -> Option<Kw> {
        Kw::ALL.iter().find(|(n, _)| *n == s).map(|x| x.1)
    }

    pub fn name(self) -> &'static str {
        Kw::ALL
            .iter()
            .find(|(_, k)| *k == self)
            .map(|x| x.0)
            .unwrap()
    }
}

fn qualified(q: &Option<String>, n: &str) -> String {
    match q {
        Some(q) => format!("{q}.{n}"),
        None => n.to_string(),
    }
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::VarId(q, n) | Tok::ConId(q, n) | Tok::VarSym(q, n) | Tok::ConSym(q, n) => {
                format!("`{}`", qualified(q, n))
            }
            Tok::Int(n) => format!("integer `{n}`"),
            Tok::Float(x) => format!("number `{x}`"),
            Tok::Char(_) => "character literal".into(),
            Tok::Str(_) => "string literal".into(),
            Tok::Kw(k) => format!("keyword `{}`", k.name()),
            Tok::DotDot => "`..`".into(),
            Tok::DColon => "`::`".into(),
            Tok::Equals => "`=`".into(),
            Tok::Backslash => "`\\`".into(),
            Tok::Bar => "`|`".into(),
            Tok::LArrow => "`<-`".into(),
            Tok::RArrow => "`->`".into(),
            Tok::At => "`@`".into(),
            Tok::Tilde => "`~`".into(),
            Tok::FatArrow => "`=>`".into(),
            Tok::LParen => "`(`".into(),
            Tok::RParen => "`)`".into(),
            Tok::LBracket => "`[`".into(),
            Tok::RBracket => "`]`".into(),
            Tok::LBrace => "`{`".into(),
            Tok::RBrace => "`}`".into(),
            Tok::Comma => "`,`".into(),
            Tok::Semi => "`;`".into(),
            Tok::Backtick => "`` ` ``".into(),
            Tok::Underscore => "`_`".into(),
            Tok::VLBrace => "start of a layout block".into(),
            Tok::VSemi => "new line".into(),
            Tok::VRBrace => "end of a layout block".into(),
            Tok::Eof => "end of file".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// 1-based line and column (tabs stop every 8 columns, Report §10.3).
    pub line: u32,
    pub col: u32,
}

/// Result of lexing a module.
#[derive(Debug, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub errors: Vec<Diagnostic>,
    /// Extensions named in `{-# LANGUAGE ... #-}` pragmas.
    pub extensions: Vec<(String, Span)>,
}

const SYMBOL: &str = "!#$%&*+./<=>?@\\^|-~:";

fn is_symbol(c: char) -> bool {
    SYMBOL.contains(c) || (!c.is_ascii() && (c.is_ascii_punctuation() || is_unicode_symbol(c)))
}

fn is_unicode_symbol(c: char) -> bool {
    // Mathematical operators and arrows, the common Unicode symbol blocks.
    matches!(c as u32, 0x2190..=0x21FF | 0x2200..=0x22FF | 0x27C0..=0x27EF | 0x2980..=0x29FF)
}

fn is_ident_start_lower(c: char) -> bool {
    c == '_' || c.is_lowercase() || (c.is_alphabetic() && !c.is_uppercase())
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

struct Lexer {
    file: FileId,
    chars: Vec<(usize, char)>,
    len: usize,
    i: usize,
    line: u32,
    col: u32,
    out: Lexed,
}

impl Lexer {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).map(|x| x.1)
    }

    fn pos(&self) -> u32 {
        self.chars.get(self.i).map(|x| x.0).unwrap_or(self.len) as u32
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.i += 1;
        match c {
            '\n' => {
                self.line += 1;
                self.col = 1;
            }
            '\t' => self.col = ((self.col - 1) / 8 + 1) * 8 + 1,
            _ => self.col += 1,
        }
        Some(c)
    }

    fn span(&self, start: u32) -> Span {
        Span::new(self.file, start, self.pos())
    }

    fn error(&mut self, span: Span, msg: impl Into<String>, label: impl Into<String>) {
        self.out.errors.push(
            Diagnostic::error(msg)
                .with_code("E0101")
                .with_label(span, label),
        );
    }

    fn push(&mut self, tok: Tok, start: u32, line: u32, col: u32) {
        let span = self.span(start);
        self.out.tokens.push(Token {
            tok,
            span,
            line,
            col,
        });
    }

    fn run(&mut self) {
        loop {
            self.skip_trivia();
            let (start, line, col) = (self.pos(), self.line, self.col);
            let Some(c) = self.peek(0) else {
                self.push(Tok::Eof, start, line, col);
                return;
            };
            let tok = if c.is_uppercase() {
                self.name()
            } else if is_ident_start_lower(c) {
                let s = self.ident();
                if s == "_" {
                    Tok::Underscore
                } else if let Some(k) = Kw::from_name(&s) {
                    Tok::Kw(k)
                } else {
                    Tok::VarId(None, s)
                }
            } else if c.is_ascii_digit() {
                self.number(start)
            } else if c == '\'' {
                self.char_lit(start)
            } else if c == '"' {
                self.string_lit(start)
            } else if is_symbol(c) {
                let s = self.symbol();
                sym_token(None, s)
            } else {
                self.bump();
                match c {
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    '[' => Tok::LBracket,
                    ']' => Tok::RBracket,
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    ',' => Tok::Comma,
                    ';' => Tok::Semi,
                    '`' => Tok::Backtick,
                    _ => {
                        let sp = self.span(start);
                        self.error(sp, format!("unexpected character `{c}`"), "not valid here");
                        continue;
                    }
                }
            };
            self.push(tok, start, line, col);
        }
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if !is_ident_char(c) {
                break;
            }
            s.push(c);
            self.bump();
        }
        s
    }

    fn symbol(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if !is_symbol(c) {
                break;
            }
            s.push(c);
            self.bump();
        }
        s
    }

    /// A constructor name, or a qualified name (`M.N.x`, `M.+`, `M.C`).
    fn name(&mut self) -> Tok {
        let mut quals: Vec<String> = Vec::new();
        loop {
            let con = self.ident();
            // A qualifier continues with `.` followed by a name or symbol.
            if self.peek(0) == Some('.')
                && let Some(n) = self.peek(1)
            {
                let q = || {
                    let mut v = quals.clone();
                    v.push(con.clone());
                    Some(v.join("."))
                };
                if n.is_uppercase() {
                    self.bump();
                    quals.push(con);
                    continue;
                }
                if is_ident_start_lower(n) {
                    // `M.where` is not a qualified name (Report §2.4).
                    let save = (self.i, self.line, self.col);
                    self.bump();
                    let v = self.ident();
                    if Kw::from_name(&v).is_none() && v != "_" {
                        return Tok::VarId(q(), v);
                    }
                    (self.i, self.line, self.col) = save;
                } else if is_symbol(n) {
                    self.bump();
                    let s = self.symbol();
                    return sym_token(q(), s);
                }
            }
            let q = if quals.is_empty() {
                None
            } else {
                Some(quals.join("."))
            };
            return Tok::ConId(q, con);
        }
    }

    fn digits(&mut self, radix: u32) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_digit(radix) {
                s.push(c);
                self.bump();
            } else if c == '_' && self.peek(1).is_some_and(|d| d.is_digit(radix)) {
                self.bump();
            } else {
                break;
            }
        }
        s
    }

    fn number(&mut self, start: u32) -> Tok {
        if self.peek(0) == Some('0')
            && let Some(x) = self.peek(1)
        {
            let radix = match x {
                'x' | 'X' => 16,
                'o' | 'O' => 8,
                'b' | 'B' => 2,
                _ => 0,
            };
            if radix != 0 && self.peek(2).is_some_and(|d| d.is_digit(radix)) {
                self.bump();
                self.bump();
                let d = self.digits(radix);
                return match u128::from_str_radix(&d, radix) {
                    Ok(n) => Tok::Int(n),
                    Err(_) => {
                        let sp = self.span(start);
                        self.error(sp, "integer literal is too large", "too large");
                        Tok::Int(0)
                    }
                };
            }
        }
        let mut text = self.digits(10);
        let mut float = false;
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            text.push('.');
            text.push_str(&self.digits(10));
            float = true;
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let k = if matches!(self.peek(1), Some('+' | '-')) {
                2
            } else {
                1
            };
            if self.peek(k).is_some_and(|c| c.is_ascii_digit()) {
                text.push('e');
                self.bump();
                if k == 2 {
                    text.push(self.bump().unwrap());
                }
                text.push_str(&self.digits(10));
                float = true;
            }
        }
        if float {
            Tok::Float(text.parse().unwrap_or(0.0))
        } else {
            match text.parse::<u128>() {
                Ok(n) => Tok::Int(n),
                Err(_) => {
                    let sp = self.span(start);
                    self.error(sp, "integer literal is too large", "too large");
                    Tok::Int(0)
                }
            }
        }
    }

    /// One character of a character or string literal; `None` for `\&`
    /// and string gaps.
    fn lit_char(&mut self, quote: char) -> Result<Option<char>, String> {
        let c = self.bump().ok_or("unterminated literal")?;
        if c == '\n' {
            return Err("new line in a literal".into());
        }
        if c != '\\' {
            return Ok(Some(c));
        }
        let e = self.bump().ok_or("unterminated escape")?;
        let simple = match e {
            'a' => Some('\x07'),
            'b' => Some('\x08'),
            'f' => Some('\x0c'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'v' => Some('\x0b'),
            '\\' => Some('\\'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };
        if let Some(s) = simple {
            return Ok(Some(s));
        }
        if e == '&' && quote == '"' {
            return Ok(None);
        }
        if e.is_whitespace() && quote == '"' {
            // String gap: `\  ...  \`.
            while let Some(c) = self.bump() {
                if c == '\\' {
                    return Ok(None);
                }
                if !c.is_whitespace() {
                    return Err("invalid string gap".into());
                }
            }
            return Err("unterminated string gap".into());
        }
        let num = |l: &mut Lexer, radix: u32| -> Result<Option<char>, String> {
            let d = l.digits(radix);
            let n = u32::from_str_radix(&d, radix).map_err(|_| "invalid escape".to_string())?;
            char::from_u32(n)
                .map(Some)
                .ok_or_else(|| "character code out of range".into())
        };
        match e {
            '0'..='9' => {
                self.i -= 1;
                self.col -= 1;
                num(self, 10)
            }
            'x' => num(self, 16),
            'o' => num(self, 8),
            '^' => {
                let c = self.bump().ok_or("unterminated escape")?;
                if ('@'..='_').contains(&c) {
                    Ok(char::from_u32(c as u32 - 64))
                } else {
                    Err("invalid control escape".into())
                }
            }
            _ => {
                // ASCII control names (`\NUL`, `\SOH`, ..., `\DEL`).
                const NAMES: [&str; 34] = [
                    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT",
                    "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB",
                    "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US", "SP", "DEL",
                ];
                let mut name = String::from(e);
                while name.len() < 3
                    && let Some(c) = self.peek(0)
                    && c.is_ascii_uppercase()
                {
                    // `\SO` followed by `H` is `\SOH` (longest match).
                    let mut longer = name.clone();
                    longer.push(c);
                    if !NAMES.iter().any(|n| n.starts_with(&longer)) {
                        break;
                    }
                    name = longer;
                    self.bump();
                }
                match NAMES.iter().position(|n| *n == name) {
                    Some(32) => Ok(Some(' ')),
                    Some(33) => Ok(Some('\x7f')),
                    Some(k) => Ok(char::from_u32(k as u32)),
                    None => Err(format!("unknown escape `\\{name}`")),
                }
            }
        }
    }

    fn char_lit(&mut self, start: u32) -> Tok {
        self.bump();
        let c = match self.lit_char('\'') {
            Ok(Some(c)) => c,
            Ok(None) => '\0',
            Err(e) => {
                let sp = self.span(start);
                self.error(sp, format!("invalid character literal: {e}"), "here");
                return Tok::Char('\0');
            }
        };
        if self.peek(0) == Some('\'') {
            self.bump();
        } else {
            let sp = self.span(start);
            self.error(sp, "unterminated character literal", "expected `'`");
        }
        Tok::Char(c)
    }

    fn string_lit(&mut self, start: u32) -> Tok {
        self.bump();
        let mut s = String::new();
        loop {
            match self.peek(0) {
                Some('"') => {
                    self.bump();
                    return Tok::Str(s);
                }
                None | Some('\n') => {
                    let sp = self.span(start);
                    self.error(sp, "unterminated string literal", "string starts here");
                    return Tok::Str(s);
                }
                _ => match self.lit_char('"') {
                    Ok(Some(c)) => s.push(c),
                    Ok(None) => {}
                    Err(e) => {
                        let sp = self.span(start);
                        self.error(sp, format!("invalid string literal: {e}"), "here");
                        // Skip to the end of the string.
                        while let Some(c) = self.peek(0) {
                            if c == '"' || c == '\n' {
                                break;
                            }
                            self.bump();
                        }
                        if self.peek(0) == Some('"') {
                            self.bump();
                        }
                        return Tok::Str(s);
                    }
                },
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek(0) {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('-') if self.peek(1) == Some('-') => {
                    let mut k = 2;
                    while self.peek(k) == Some('-') {
                        k += 1;
                    }
                    if self.peek(k).is_some_and(is_symbol) {
                        return; // an operator such as `-->`
                    }
                    while let Some(c) = self.peek(0) {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('{') if self.peek(1) == Some('-') => self.block_comment(),
                _ => return,
            }
        }
    }

    fn block_comment(&mut self) {
        let start = self.pos();
        let pragma = self.peek(2) == Some('#');
        self.bump();
        self.bump();
        let mut depth = 1;
        let mut text = String::new();
        while depth > 0 {
            match (self.peek(0), self.peek(1)) {
                (None, _) => {
                    let sp = Span::new(self.file, start, start + 2);
                    self.error(sp, "unterminated block comment", "comment starts here");
                    return;
                }
                (Some('{'), Some('-')) => {
                    self.bump();
                    self.bump();
                    depth += 1;
                }
                (Some('-'), Some('}')) => {
                    self.bump();
                    self.bump();
                    depth -= 1;
                }
                (Some(c), _) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
        if pragma {
            let body = text.trim_start_matches('#').trim_end_matches('#').trim();
            let mut words = body.splitn(2, char::is_whitespace);
            if words
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("LANGUAGE"))
            {
                let sp = self.span(start);
                for e in words.next().unwrap_or("").split(',') {
                    let e = e.trim();
                    if !e.is_empty() {
                        self.out.extensions.push((e.to_string(), sp));
                    }
                }
            }
        }
    }
}

fn sym_token(q: Option<String>, s: String) -> Tok {
    if q.is_none() {
        match s.as_str() {
            ".." => return Tok::DotDot,
            "::" => return Tok::DColon,
            "=" => return Tok::Equals,
            "\\" => return Tok::Backslash,
            "|" => return Tok::Bar,
            "<-" => return Tok::LArrow,
            "->" => return Tok::RArrow,
            "@" => return Tok::At,
            "~" => return Tok::Tilde,
            "=>" => return Tok::FatArrow,
            _ => {}
        }
    }
    if s.starts_with(':') {
        Tok::ConSym(q, s)
    } else {
        Tok::VarSym(q, s)
    }
}

/// Lexes a whole module.
pub fn lex(file: FileId, src: &str) -> Lexed {
    let mut l = Lexer {
        file,
        chars: src.char_indices().collect(),
        len: src.len(),
        i: 0,
        line: 1,
        col: 1,
        out: Lexed::default(),
    };
    l.run();
    l.out
}
