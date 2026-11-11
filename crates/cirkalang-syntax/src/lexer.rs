//! Lexical analysis (spec-v0.0.1.md §2).

use cirka_diagnostics::{Diagnostic, FileId, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// Lower-case identifier, possibly qualified (`A.B.x`).
    LIdent(Vec<String>, String),
    /// Upper-case identifier, possibly qualified (`A.B.C`).
    UIdent(Vec<String>, String),
    /// Operator, possibly qualified (`A.+`).
    Op(Vec<String>, String),
    Int(u128),
    Float(f64),
    Char(char),
    Str(String),
    Hole(String),
    Kw(Kw),
    // Reserved symbols.
    Colon,
    Equals,
    FatArrow,
    Arrow,
    Backslash,
    Bar,
    LArrow,
    At,
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
    Module,
    Import,
    Data,
    Where,
    Let,
    In,
    Case,
    Of,
    If,
    Then,
    Else,
    Do,
    Private,
    Infix,
    Infixl,
    Infixr,
    Prim,
    Total,
    Partial,
    Covering,
    Trait,
    Impl,
    Mutual,
    Deriving,
}

impl Kw {
    fn from_str(s: &str) -> Option<Kw> {
        use Kw::*;
        Some(match s {
            "module" => Module,
            "import" => Import,
            "data" => Data,
            "where" => Where,
            "let" => Let,
            "in" => In,
            "case" => Case,
            "of" => Of,
            "if" => If,
            "then" => Then,
            "else" => Else,
            "do" => Do,
            "private" => Private,
            "infix" => Infix,
            "infixl" => Infixl,
            "infixr" => Infixr,
            "prim" => Prim,
            "total" => Total,
            "partial" => Partial,
            "covering" => Covering,
            "trait" => Trait,
            "impl" => Impl,
            "mutual" => Mutual,
            "deriving" => Deriving,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        use Kw::*;
        match self {
            Module => "module",
            Import => "import",
            Data => "data",
            Where => "where",
            Let => "let",
            In => "in",
            Case => "case",
            Of => "of",
            If => "if",
            Then => "then",
            Else => "else",
            Do => "do",
            Private => "private",
            Infix => "infix",
            Infixl => "infixl",
            Infixr => "infixr",
            Prim => "prim",
            Total => "total",
            Partial => "partial",
            Covering => "covering",
            Trait => "trait",
            Impl => "impl",
            Mutual => "mutual",
            Deriving => "deriving",
        }
    }
}

impl Tok {
    pub fn describe(&self) -> String {
        match self {
            Tok::LIdent(m, n) | Tok::UIdent(m, n) | Tok::Op(m, n) => {
                let mut s = String::from("`");
                for x in m {
                    s.push_str(x);
                    s.push('.');
                }
                s.push_str(n);
                s.push('`');
                s
            }
            Tok::Int(n) => format!("integer `{n}`"),
            Tok::Float(x) => format!("number `{x}`"),
            Tok::Char(_) => "character literal".into(),
            Tok::Str(_) => "string literal".into(),
            Tok::Hole(h) => format!("hole `?{h}`"),
            Tok::Kw(k) => format!("keyword `{}`", k.name()),
            Tok::Colon => "`:`".into(),
            Tok::Equals => "`=`".into(),
            Tok::FatArrow => "`=>`".into(),
            Tok::Arrow => "`->`".into(),
            Tok::Backslash => "`\\`".into(),
            Tok::Bar => "`|`".into(),
            Tok::LArrow => "`<-`".into(),
            Tok::At => "`@`".into(),
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
            Tok::VLBrace => "start of a block".into(),
            Tok::VSemi => "new line".into(),
            Tok::VRBrace => "end of a block".into(),
            Tok::Eof => "end of file".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    /// 1-based line and column of the first character.
    pub line: u32,
    pub col: u32,
}

const SYMBOL: &str = "!#$%&*+./<=>?@\\^|-~:";

fn is_symbol(c: char) -> bool {
    SYMBOL.contains(c)
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '\''
}

struct Lexer<'s> {
    src: &'s str,
    chars: Vec<(usize, char)>,
    i: usize,
    file: FileId,
    line: u32,
    line_start: usize,
    toks: Vec<Token>,
    errs: Vec<Diagnostic>,
}

/// Splits `src` into tokens (without layout tokens). Lexical errors are
/// reported and the offending characters skipped.
pub fn lex(file: FileId, src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut l = Lexer {
        src,
        chars: src.char_indices().collect(),
        i: 0,
        file,
        line: 1,
        line_start: 0,
        toks: Vec::new(),
        errs: Vec::new(),
    };
    l.run();
    let end = src.len() as u32;
    l.toks.push(Token {
        tok: Tok::Eof,
        span: Span::new(file, end, end),
        line: l.line + 1,
        col: 0,
    });
    (l.toks, l.errs)
}

impl Lexer<'_> {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).map(|x| x.1)
    }

    fn pos(&self) -> usize {
        self.chars
            .get(self.i)
            .map(|x| x.0)
            .unwrap_or(self.src.len())
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.line_start = self.pos();
        }
        Some(c)
    }

    fn col_of(&self, byte: usize) -> u32 {
        // Columns count characters; a tab advances to the next multiple of 8.
        let mut col = 1u32;
        for c in self.src[self.line_start..byte].chars() {
            if c == '\t' {
                col = ((col - 1) / 8 + 1) * 8 + 1;
            } else {
                col += 1;
            }
        }
        col
    }

    fn error(&mut self, start: usize, end: usize, msg: impl Into<String>, label: &str) {
        self.errs.push(
            Diagnostic::error(msg)
                .with_code("E1001")
                .with_label(Span::new(self.file, start as u32, end as u32), label),
        );
    }

    fn push(&mut self, tok: Tok, start: usize, line: u32, col: u32) {
        let end = self.pos();
        self.toks.push(Token {
            tok,
            span: Span::new(self.file, start as u32, end as u32),
            line,
            col,
        });
    }

    fn run(&mut self) {
        while let Some(c) = self.peek(0) {
            let start = self.pos();
            let line = self.line;
            let col = self.col_of(start);
            if c.is_whitespace() {
                self.bump();
                continue;
            }
            if c == '-' && self.peek(1) == Some('-') {
                // `--` comment unless the symbol run has other characters.
                let mut k = 0;
                while self.peek(k).is_some_and(is_symbol) {
                    k += 1;
                }
                let run: String = (0..k).filter_map(|j| self.peek(j)).collect();
                if run.chars().all(|x| x == '-') {
                    while self.peek(0).is_some_and(|x| x != '\n') {
                        self.bump();
                    }
                    continue;
                }
            }
            if c == '{' && self.peek(1) == Some('-') {
                self.block_comment(start);
                continue;
            }
            if c.is_ascii_digit() {
                let t = self.number(start);
                self.push(t, start, line, col);
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let t = self.ident(start);
                self.push(t, start, line, col);
                continue;
            }
            match c {
                '\'' => {
                    self.bump();
                    let t = match self.escaped_char(start, '\'') {
                        Some(ch) => {
                            if self.peek(0) == Some('\'') {
                                self.bump();
                            } else {
                                self.error(
                                    start,
                                    self.pos(),
                                    "unterminated character literal",
                                    "expected `'`",
                                );
                            }
                            Tok::Char(ch)
                        }
                        None => Tok::Char('\0'),
                    };
                    self.push(t, start, line, col);
                    continue;
                }
                '"' => {
                    self.bump();
                    let mut s = String::new();
                    loop {
                        match self.peek(0) {
                            None | Some('\n') => {
                                self.error(
                                    start,
                                    self.pos(),
                                    "unterminated string literal",
                                    "string starts here",
                                );
                                break;
                            }
                            Some('"') => {
                                self.bump();
                                break;
                            }
                            Some(_) => {
                                if let Some(ch) = self.escaped_char(start, '"') {
                                    s.push(ch);
                                }
                            }
                        }
                    }
                    self.push(Tok::Str(s), start, line, col);
                    continue;
                }
                '?' if self
                    .peek(1)
                    .is_some_and(|x| x.is_ascii_lowercase() || x == '_') =>
                {
                    self.bump();
                    let s = self.pos();
                    while self.peek(0).is_some_and(is_ident_char) {
                        self.bump();
                    }
                    let name = self.src[s..self.pos()].to_string();
                    self.push(Tok::Hole(name), start, line, col);
                    continue;
                }
                _ => {}
            }
            let simple = match c {
                '(' => Some(Tok::LParen),
                ')' => Some(Tok::RParen),
                '[' => Some(Tok::LBracket),
                ']' => Some(Tok::RBracket),
                '{' => Some(Tok::LBrace),
                '}' => Some(Tok::RBrace),
                ',' => Some(Tok::Comma),
                ';' => Some(Tok::Semi),
                '`' => Some(Tok::Backtick),
                _ => None,
            };
            if let Some(t) = simple {
                self.bump();
                self.push(t, start, line, col);
                continue;
            }
            if is_symbol(c) {
                let s = self.pos();
                while self.peek(0).is_some_and(is_symbol) {
                    self.bump();
                }
                let op = &self.src[s..self.pos()];
                let t = match op {
                    ":" => Tok::Colon,
                    "=" => Tok::Equals,
                    "=>" => Tok::FatArrow,
                    "->" => Tok::Arrow,
                    "\\" => Tok::Backslash,
                    "|" => Tok::Bar,
                    "<-" => Tok::LArrow,
                    "@" => Tok::At,
                    _ => Tok::Op(Vec::new(), op.to_string()),
                };
                self.push(t, start, line, col);
                continue;
            }
            self.bump();
            self.error(
                start,
                self.pos(),
                format!("unexpected character `{}`", c.escape_debug()),
                "not valid here",
            );
        }
    }

    fn block_comment(&mut self, start: usize) {
        self.bump();
        self.bump();
        let mut depth = 1;
        while depth > 0 {
            match (self.peek(0), self.peek(1)) {
                (None, _) => {
                    self.error(
                        start,
                        start + 2,
                        "unterminated block comment",
                        "comment starts here",
                    );
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
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn number(&mut self, start: usize) -> Tok {
        let radix = match (self.peek(0), self.peek(1)) {
            (Some('0'), Some('x' | 'X')) => 16,
            (Some('0'), Some('b' | 'B')) => 2,
            (Some('0'), Some('o' | 'O')) => 8,
            _ => 10,
        };
        if radix != 10 {
            self.bump();
            self.bump();
        }
        let mut digits = String::new();
        while let Some(c) = self.peek(0) {
            if c == '_' {
                self.bump();
            } else if c.is_digit(radix) {
                digits.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let mut is_float = false;
        if radix == 10 {
            if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                digits.push('.');
                self.bump();
                while let Some(c) = self.peek(0) {
                    if c.is_ascii_digit() {
                        digits.push(c);
                        self.bump();
                    } else if c == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            if matches!(self.peek(0), Some('e' | 'E'))
                && (self.peek(1).is_some_and(|c| c.is_ascii_digit())
                    || (matches!(self.peek(1), Some('+' | '-'))
                        && self.peek(2).is_some_and(|c| c.is_ascii_digit())))
            {
                is_float = true;
                digits.push('e');
                self.bump();
                if let Some(s @ ('+' | '-')) = self.peek(0) {
                    digits.push(s);
                    self.bump();
                }
                while let Some(c) = self.peek(0).filter(|c| c.is_ascii_digit()) {
                    digits.push(c);
                    self.bump();
                }
            }
        }
        if self
            .peek(0)
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            let s = self.pos();
            while self.peek(0).is_some_and(is_ident_char) {
                self.bump();
            }
            self.error(
                start,
                self.pos(),
                "invalid number literal",
                "unexpected characters after the number",
            );
            let _ = s;
        }
        if is_float {
            return Tok::Float(digits.parse().unwrap_or(0.0));
        }
        if digits.is_empty() {
            self.error(
                start,
                self.pos(),
                "number literal without digits",
                "expected digits",
            );
            return Tok::Int(0);
        }
        match u128::from_str_radix(&digits, radix) {
            Ok(n) => Tok::Int(n),
            Err(_) => {
                self.error(
                    start,
                    self.pos(),
                    "integer literal is too large",
                    "does not fit in 128 bits",
                );
                Tok::Int(0)
            }
        }
    }

    /// An identifier, possibly qualified: `A.B.x`, `A.B`, `A.+`.
    fn ident(&mut self, start: usize) -> Tok {
        let mut path = Vec::new();
        loop {
            let s = self.pos();
            while self.peek(0).is_some_and(is_ident_char) {
                self.bump();
            }
            let part = self.src[s..self.pos()].to_string();
            let upper = part.starts_with(|c: char| c.is_uppercase());
            if upper && self.peek(0) == Some('.') {
                match self.peek(1) {
                    Some(c) if c.is_alphabetic() || c == '_' => {
                        path.push(part);
                        self.bump();
                        continue;
                    }
                    Some(c) if is_symbol(c) && c != '.' => {
                        path.push(part);
                        self.bump();
                        let s = self.pos();
                        while self.peek(0).is_some_and(is_symbol) {
                            self.bump();
                        }
                        return Tok::Op(path, self.src[s..self.pos()].to_string());
                    }
                    _ => {}
                }
            }
            let _ = start;
            if path.is_empty() && part == "_" {
                return Tok::Underscore;
            }
            if path.is_empty()
                && let Some(k) = Kw::from_str(&part)
            {
                return Tok::Kw(k);
            }
            return if upper {
                Tok::UIdent(path, part)
            } else {
                Tok::LIdent(path, part)
            };
        }
    }

    /// One (possibly escaped) character of a char/string literal.
    fn escaped_char(&mut self, start: usize, quote: char) -> Option<char> {
        let c = self.bump()?;
        if c == '\n' {
            self.error(
                start,
                self.pos(),
                "unterminated literal",
                "literal starts here",
            );
            return None;
        }
        if c == quote && quote == '\'' {
            self.error(
                start,
                self.pos(),
                "empty character literal",
                "write a character between the quotes",
            );
            return None;
        }
        if c != '\\' {
            return Some(c);
        }
        let e = self.pos();
        let Some(k) = self.bump() else {
            self.error(
                start,
                self.pos(),
                "unterminated literal",
                "literal starts here",
            );
            return None;
        };
        let r = match k {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'x' => {
                let h: String = (0..2)
                    .filter_map(|_| {
                        self.peek(0).filter(|c| c.is_ascii_hexdigit()).inspect(|_| {
                            self.bump();
                        })
                    })
                    .collect();
                match u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    Some(c) if h.len() == 2 => c,
                    _ => {
                        self.error(
                            e - 1,
                            self.pos(),
                            "invalid `\\x` escape",
                            "expected two hex digits",
                        );
                        return None;
                    }
                }
            }
            'u' => {
                if self.peek(0) != Some('{') {
                    self.error(
                        e - 1,
                        self.pos(),
                        "invalid `\\u` escape",
                        "expected `\\u{...}`",
                    );
                    return None;
                }
                self.bump();
                let mut h = String::new();
                while let Some(c) = self.peek(0).filter(|c| c.is_ascii_hexdigit()) {
                    h.push(c);
                    self.bump();
                }
                let closed = self.peek(0) == Some('}');
                if closed {
                    self.bump();
                }
                match u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    Some(c) if closed && h.len() <= 6 => c,
                    _ => {
                        self.error(
                            e - 1,
                            self.pos(),
                            "invalid `\\u{...}` escape",
                            "not a Unicode scalar value",
                        );
                        return None;
                    }
                }
            }
            other => {
                self.error(
                    e - 1,
                    self.pos(),
                    format!("unknown escape `\\{other}`"),
                    "unknown escape",
                );
                return None;
            }
        };
        Some(r)
    }
}
