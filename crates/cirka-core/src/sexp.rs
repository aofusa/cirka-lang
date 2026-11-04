//! S-expression reader used by the Core text format.

use cirka_diagnostics::{Diagnostic, FileId, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    Sym(String, Span),
    Int(i64, Span),
    Float(f64, Span),
    Str(Vec<u8>, Span),
    List(Vec<Sexp>, Span),
}

impl Sexp {
    pub fn span(&self) -> Span {
        match self {
            Sexp::Sym(_, s)
            | Sexp::Int(_, s)
            | Sexp::Float(_, s)
            | Sexp::Str(_, s)
            | Sexp::List(_, s) => *s,
        }
    }

    pub fn sym(&self) -> Option<&str> {
        match self {
            Sexp::Sym(s, _) => Some(s),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::List(l, _) => Some(l),
            _ => None,
        }
    }

    /// The head symbol of a list (`(head ...)`).
    pub fn head(&self) -> Option<&str> {
        self.list().and_then(|l| l.first()).and_then(|h| h.sym())
    }
}

/// Reads all top-level S-expressions of a file. `;` starts a comment.
pub fn read(file: FileId, src: &str) -> Result<Vec<Sexp>, Diagnostic> {
    let b = src.as_bytes();
    let mut pos = 0usize;
    let mut stack: Vec<(Vec<Sexp>, usize)> = vec![(Vec::new(), 0)];
    let span = |s: usize, e: usize| Span::new(file, s as u32, e as u32);
    while pos < b.len() {
        let c = b[pos];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
            b';' => {
                while pos < b.len() && b[pos] != b'\n' {
                    pos += 1;
                }
            }
            b'(' | b'[' => {
                stack.push((Vec::new(), pos));
                pos += 1;
            }
            b')' | b']' => {
                if stack.len() == 1 {
                    return Err(Diagnostic::error("unbalanced `)`")
                        .with_code("C0001")
                        .with_label(span(pos, pos + 1), "no matching `(`"));
                }
                let (items, start) = stack.pop().unwrap();
                stack
                    .last_mut()
                    .unwrap()
                    .0
                    .push(Sexp::List(items, span(start, pos + 1)));
                pos += 1;
            }
            b'"' => {
                let start = pos;
                pos += 1;
                let mut out = Vec::new();
                loop {
                    let Some(&c) = b.get(pos) else {
                        return Err(Diagnostic::error("unterminated string")
                            .with_code("C0001")
                            .with_label(span(start, pos), "string starts here"));
                    };
                    pos += 1;
                    match c {
                        b'"' => break,
                        b'\\' => {
                            let e = b.get(pos).copied().unwrap_or(b'\\');
                            pos += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'r' => out.push(b'\r'),
                                b'0' => out.push(0),
                                b'x' => {
                                    let h = std::str::from_utf8(b.get(pos..pos + 2).unwrap_or(b""))
                                        .ok()
                                        .and_then(|h| u8::from_str_radix(h, 16).ok());
                                    match h {
                                        Some(v) => {
                                            out.push(v);
                                            pos += 2;
                                        }
                                        None => {
                                            return Err(Diagnostic::error("bad `\\x` escape")
                                                .with_code("C0001")
                                                .with_label(
                                                    span(pos - 2, pos),
                                                    "expected two hex digits",
                                                ));
                                        }
                                    }
                                }
                                other => out.push(other),
                            }
                        }
                        c => out.push(c),
                    }
                }
                stack
                    .last_mut()
                    .unwrap()
                    .0
                    .push(Sexp::Str(out, span(start, pos)));
            }
            _ => {
                let start = pos;
                while pos < b.len()
                    && !matches!(
                        b[pos],
                        b' ' | b'\t' | b'\n' | b'\r' | b'(' | b')' | b'[' | b']' | b';' | b'"'
                    )
                {
                    pos += 1;
                }
                let tok = &src[start..pos];
                let sp = span(start, pos);
                let atom = if let Ok(n) = tok.parse::<i64>() {
                    Sexp::Int(n, sp)
                } else if tok.starts_with(|c: char| c.is_ascii_digit() || c == '-')
                    && tok.len() > 1
                    && let Ok(x) = tok.parse::<f64>()
                {
                    Sexp::Float(x, sp)
                } else {
                    Sexp::Sym(tok.to_string(), sp)
                };
                stack.last_mut().unwrap().0.push(atom);
            }
        }
    }
    if stack.len() > 1 {
        let start = stack.last().unwrap().1;
        return Err(Diagnostic::error("unclosed `(`")
            .with_code("C0001")
            .with_label(span(start, start + 1), "this list is never closed"));
    }
    Ok(stack.pop().unwrap().0)
}

/// Escapes bytes for a string literal.
pub fn escape(b: &[u8]) -> String {
    let mut s = String::from("\"");
    for &c in b {
        match c {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            b'\r' => s.push_str("\\r"),
            0x20..=0x7e => s.push(c as char),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s.push('"');
    s
}
