//! cirka-diagnostics: source locations and compiler diagnostics shared by
//! both front ends (aCHC and CirkaLang) and the common back end.

use std::fmt::Write as _;

/// Identifies a file registered in a [`SourceMap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct FileId(pub u32);

/// A byte range in a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Span {
        Span { file, start, end }
    }

    /// A placeholder span for compiler-generated code.
    pub const DUMMY: Span = Span {
        file: FileId(u32::MAX),
        start: 0,
        end: 0,
    };

    pub fn is_dummy(&self) -> bool {
        self.file.0 == u32::MAX
    }

    /// The smallest span covering both spans (which must be in the same file).
    pub fn to(self, other: Span) -> Span {
        if self.is_dummy() {
            return other;
        }
        if other.is_dummy() || other.file != self.file {
            return self;
        }
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// One source file.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> SourceFile {
        let text = text.into();
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        SourceFile {
            name: name.into(),
            text,
            line_starts,
        }
    }

    /// 0-based line index containing byte offset `pos`.
    pub fn line_index(&self, pos: u32) -> usize {
        match self.line_starts.binary_search(&pos) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// 1-based (line, column). Columns count Unicode scalar values.
    pub fn line_col(&self, pos: u32) -> (usize, usize) {
        let pos = pos.min(self.text.len() as u32);
        let line = self.line_index(pos);
        let start = self.line_starts[line] as usize;
        let mut end = pos as usize;
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        let col = self.text[start..end].chars().count() + 1;
        (line + 1, col)
    }

    /// Text of a 0-based line without the trailing newline.
    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line] as usize;
        let end = self
            .line_starts
            .get(line + 1)
            .map(|&e| e as usize)
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_start(&self, line: usize) -> u32 {
        self.line_starts[line]
    }
}

/// All source files of a compilation.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> SourceMap {
        SourceMap::default()
    }

    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> FileId {
        self.files.push(SourceFile::new(name, text));
        FileId(self.files.len() as u32 - 1)
    }

    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    pub fn text(&self, span: Span) -> &str {
        match self.get(span.file) {
            Some(f) => &f.text[span.start as usize..span.end as usize],
            None => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Bug,
    Error,
    Warning,
    Note,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Severity::Bug => "internal compiler error",
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Option<String>,
    pub severity: Severity,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code: None,
            severity,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Warning, message)
    }

    /// An internal compiler error.
    pub fn ice(message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Bug, message)
            .with_note("this is a bug in the compiler; please report it with the input program")
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Diagnostic {
        self.code = Some(code.into());
        self
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
            primary: true,
        });
        self
    }

    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label {
            span,
            message: message.into(),
            primary: false,
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Diagnostic {
        self.help.push(help.into());
        self
    }

    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error | Severity::Bug)
    }

    /// The span of the first primary label.
    pub fn primary_span(&self) -> Option<Span> {
        self.labels.iter().find(|l| l.primary).map(|l| l.span)
    }
}

/// A list of diagnostics produced by a pass.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    pub list: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Diagnostics {
        Diagnostics::default()
    }
    pub fn push(&mut self, d: Diagnostic) {
        self.list.push(d);
    }
    pub fn extend(&mut self, other: Diagnostics) {
        self.list.extend(other.list);
    }
    pub fn has_errors(&self) -> bool {
        self.list.iter().any(|d| d.is_error())
    }
    pub fn error_count(&self) -> usize {
        self.list.iter().filter(|d| d.is_error()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
    pub fn render(&self, sm: &SourceMap, style: Style) -> String {
        let mut s = String::new();
        for d in &self.list {
            s.push_str(&render(d, sm, style));
        }
        s
    }
}

/// Output style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Plain text, stable for golden tests.
    Plain,
    /// ANSI colors for terminals.
    Color,
}

impl Style {
    /// Colors when stderr is a terminal and `NO_COLOR` is unset.
    pub fn for_stderr() -> Style {
        use std::io::IsTerminal;
        if std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal() {
            Style::Color
        } else {
            Style::Plain
        }
    }
}

/// Display width of a character in a monospace terminal (East Asian wide
/// characters take two columns).
pub fn char_width(c: char) -> usize {
    let c = c as u32;
    let wide = matches!(c,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF |
        0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF |
        0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F |
        0x1F900..=0x1F9FF | 0x20000..=0x3FFFD);
    if c == 0 {
        0
    } else if wide {
        2
    } else {
        1
    }
}

fn str_width(s: &str) -> usize {
    s.chars()
        .map(|c| if c == '\t' { 4 } else { char_width(c) })
        .sum()
}

fn paint(style: Style, code: &str, text: &str) -> String {
    match style {
        Style::Plain => text.to_string(),
        Style::Color => format!("\x1b[{code}m{text}\x1b[0m"),
    }
}

/// Renders one diagnostic (rustc-like layout).
pub fn render(d: &Diagnostic, sm: &SourceMap, style: Style) -> String {
    let mut out = String::new();
    let sev_color = match d.severity {
        Severity::Bug | Severity::Error => "1;31",
        Severity::Warning => "1;33",
        Severity::Note => "1;36",
    };
    let head = match &d.code {
        Some(c) => format!("{}[{}]", d.severity.as_str(), c),
        None => d.severity.as_str().to_string(),
    };
    let _ = writeln!(
        out,
        "{}: {}",
        paint(style, sev_color, &head),
        paint(style, "1", &d.message)
    );

    // Group labels by file, keeping first-appearance order.
    let mut files: Vec<FileId> = Vec::new();
    for l in &d.labels {
        if sm.get(l.span.file).is_some() && !files.contains(&l.span.file) {
            files.push(l.span.file);
        }
    }
    // Gutter width = widest line number of all labels.
    let mut max_line = 1;
    for l in &d.labels {
        if let Some(f) = sm.get(l.span.file) {
            max_line = max_line.max(f.line_col(l.span.end).0);
        }
    }
    let gw = max_line.to_string().len();
    let pad = " ".repeat(gw);
    let blue = |s: &str| paint(style, "1;34", s);
    for (fi, file_id) in files.iter().enumerate() {
        let f = sm.file(*file_id);
        let labels: Vec<&Label> = d
            .labels
            .iter()
            .filter(|l| l.span.file == *file_id)
            .collect();
        let first = labels
            .iter()
            .find(|l| l.primary)
            .copied()
            .unwrap_or(labels[0]);
        let (line, col) = f.line_col(first.span.start);
        let arrow = if fi == 0 { "-->" } else { ":::" };
        let _ = writeln!(out, "{pad}{} {}:{}:{}", blue(arrow), f.name, line, col);
        let _ = writeln!(out, "{pad} {}", blue("|"));
        // Lines to show, sorted.
        let mut lines: Vec<usize> = labels.iter().map(|l| f.line_index(l.span.start)).collect();
        lines.sort();
        lines.dedup();
        let mut prev: Option<usize> = None;
        for li in lines {
            if let Some(p) = prev
                && li > p + 1
            {
                let _ = writeln!(out, "{}", blue("..."));
            }
            prev = Some(li);
            let text = f.line_text(li);
            let _ = writeln!(
                out,
                "{} {} {}",
                blue(&format!("{:>gw$}", li + 1)),
                blue("|"),
                text
            );
            let line_start = f.line_start(li) as usize;
            let mut on_line: Vec<&&Label> = labels
                .iter()
                .filter(|l| f.line_index(l.span.start) == li)
                .collect();
            on_line.sort_by_key(|l| l.span.start);
            for l in on_line {
                let s = (l.span.start as usize).max(line_start) - line_start;
                let line_end = line_start + text.len();
                let e = (l.span.end as usize)
                    .min(line_end)
                    .max(l.span.start as usize)
                    - line_start;
                let s = s.min(text.len());
                let e = e.min(text.len());
                let before = str_width(safe_slice(text, 0, s));
                let width = str_width(safe_slice(text, s, e)).max(1);
                let ch = if l.primary { "^" } else { "-" };
                let marks = format!(
                    "{}{}",
                    ch.repeat(width),
                    if l.message.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", l.message)
                    }
                );
                let color = if l.primary { sev_color } else { "1;34" };
                let _ = writeln!(
                    out,
                    "{pad} {} {}{}",
                    blue("|"),
                    " ".repeat(before),
                    paint(style, color, &marks)
                );
            }
        }
    }
    for n in &d.notes {
        let _ = writeln!(
            out,
            "{pad} {} {}: {}",
            blue("="),
            paint(style, "1", "note"),
            n
        );
    }
    for h in &d.help {
        let _ = writeln!(
            out,
            "{pad} {} {}: {}",
            blue("="),
            paint(style, "1", "help"),
            h
        );
    }
    out
}

fn safe_slice(s: &str, mut a: usize, mut b: usize) -> &str {
    while a > 0 && !s.is_char_boundary(a) {
        a -= 1;
    }
    while b < s.len() && !s.is_char_boundary(b) {
        b += 1;
    }
    &s[a..b.max(a)]
}

/// Installs a panic hook that reports panics as internal compiler errors.
pub fn install_ice_hook(tool: &'static str) {
    std::panic::set_hook(Box::new(move |info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        let loc = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        eprintln!("error: internal compiler error: {msg}{loc}");
        eprintln!("note: this is a bug in {tool}; please report it with the input program");
        if std::env::var("RUST_BACKTRACE").is_ok_and(|v| v != "0") {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_ascii_and_utf8() {
        let f = SourceFile::new("a", "ab\nあいc\n");
        assert_eq!(f.line_col(0), (1, 1));
        assert_eq!(f.line_col(3), (2, 1));
        // "あ" and "い" are 3 bytes each; 'c' is at byte 9.
        assert_eq!(f.line_col(9), (2, 3));
        assert_eq!(f.line_text(1), "あいc");
    }

    #[test]
    fn render_single_label() {
        let mut sm = SourceMap::new();
        let f = sm.add("Main.hs", "main = foo bar\n");
        let d = Diagnostic::error("variable not in scope: foo")
            .with_code("E0101")
            .with_label(Span::new(f, 7, 10), "not in scope")
            .with_help("did you mean `for`?");
        let s = render(&d, &sm, Style::Plain);
        assert_eq!(
            s,
            "error[E0101]: variable not in scope: foo\n --> Main.hs:1:8\n  |\n1 | main = foo bar\n  |        ^^^ not in scope\n  = help: did you mean `for`?\n"
        );
    }

    #[test]
    fn render_multi_label_multi_file_japanese() {
        let mut sm = SourceMap::new();
        let a = sm.add("a.cirka", "let 名前 = 1\nlet x = 名前 + y\n");
        let b = sm.add("b.cirka", "y : Int\n");
        // `名前` on line 2 starts at byte 8 of the line.
        let l2 = sm.file(a).line_start(1);
        let d = Diagnostic::error("type mismatch")
            .with_label(Span::new(a, l2 + 8, l2 + 14), "this is Int")
            .with_secondary(Span::new(a, 4, 10), "defined here")
            .with_secondary(Span::new(b, 4, 7), "declared here");
        let s = render(&d, &sm, Style::Plain);
        let expected = "error: type mismatch
 --> a.cirka:2:9
  |
1 | let 名前 = 1
  |     ---- defined here
2 | let x = 名前 + y
  |         ^^^^ this is Int
 ::: b.cirka:1:5
  |
1 | y : Int
  |     --- declared here
";
        assert_eq!(s, expected);
    }

    #[test]
    fn ice_format() {
        let sm = SourceMap::new();
        let s = render(
            &Diagnostic::ice("Core Lint failed after simplify"),
            &sm,
            Style::Plain,
        );
        assert!(s.starts_with("internal compiler error: Core Lint failed after simplify\n"));
        assert!(s.contains("= note: this is a bug in the compiler"));
    }

    #[test]
    fn span_join() {
        let a = Span::new(FileId(0), 3, 5);
        let b = Span::new(FileId(0), 8, 10);
        assert_eq!(a.to(b), Span::new(FileId(0), 3, 10));
        assert_eq!(Span::DUMMY.to(b), b);
    }
}
