//! The layout algorithm (spec-v0.0.1.md §2.1): inserts virtual braces and
//! semicolons into the token stream.

use crate::lexer::{Kw, Tok, Token};
use cirka_diagnostics::Span;

#[derive(Debug)]
enum Ctx {
    /// Implicit block at a column, opened at a bracket depth.
    Implicit { col: u32, is_let: bool, depth: u32 },
    /// Explicit `{ ... }` block.
    Explicit,
    /// `if` (until `else`) or `case` (until `of`).
    Marker(Kw),
}

/// Runs the layout algorithm. The top level is an implicit block at
/// column 1.
pub fn layout(toks: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(toks.len() * 5 / 4);
    let mut stack: Vec<Ctx> = vec![Ctx::Implicit {
        col: 1,
        is_let: false,
        depth: 0,
    }];
    let mut depth: u32 = 0;
    let mut pending: Option<bool> = None; // a block opens; true if `let`
    let mut prev_line = 0u32;
    let virt = |t: Tok, at: &Token| Token {
        tok: t,
        span: Span::new(at.span.file, at.span.start, at.span.start),
        line: at.line,
        col: at.col,
    };
    for t in toks {
        let first_on_line = t.line != prev_line;
        if let Some(is_let) = pending.take() {
            if t.tok == Tok::LBrace {
                stack.push(Ctx::Explicit);
                prev_line = t.line;
                out.push(t);
                continue;
            }
            let enclosing = stack
                .iter()
                .rev()
                .find_map(|c| match c {
                    Ctx::Implicit { col, .. } => Some(*col),
                    _ => None,
                })
                .unwrap_or(0);
            if t.tok != Tok::Eof && t.col > enclosing {
                out.push(virt(Tok::VLBrace, &t));
                stack.push(Ctx::Implicit {
                    col: t.col,
                    is_let,
                    depth,
                });
                prev_line = t.line;
                push_tok(&mut out, &mut stack, &mut depth, &mut pending, t);
                continue;
            }
            out.push(virt(Tok::VLBrace, &t));
            out.push(virt(Tok::VRBrace, &t));
        }
        if t.tok == Tok::Eof {
            while let Some(c) = stack.pop() {
                if let Ctx::Implicit { .. } = c {
                    out.push(virt(Tok::VRBrace, &t));
                }
            }
            out.push(t);
            break;
        }
        if first_on_line {
            // Close blocks indented deeper than this line.
            while let Some(top) = stack
                .iter()
                .rposition(|c| matches!(c, Ctx::Implicit { .. } | Ctx::Explicit))
            {
                match stack[top] {
                    Ctx::Implicit { col, .. } if t.col < col => {
                        stack.truncate(top);
                        out.push(virt(Tok::VRBrace, &t));
                    }
                    _ => break,
                }
            }
            if let Some(Ctx::Implicit { col, .. }) = stack
                .iter()
                .rev()
                .find(|c| matches!(c, Ctx::Implicit { .. } | Ctx::Explicit))
                && t.col == *col
                && !matches!(
                    t.tok,
                    Tok::Kw(Kw::Then | Kw::Else | Kw::Of | Kw::In)
                        | Tok::RParen
                        | Tok::RBracket
                        | Tok::RBrace
                        | Tok::Comma
                )
            {
                out.push(virt(Tok::VSemi, &t));
            }
        }
        prev_line = t.line;
        // Tokens that close implicit blocks.
        match &t.tok {
            Tok::Kw(Kw::In) => {
                if let Some(i) = stack.iter().rposition(|c| !matches!(c, Ctx::Marker(_)))
                    && let Ctx::Implicit { is_let: true, .. } = stack[i]
                {
                    stack.truncate(i);
                    out.push(virt(Tok::VRBrace, &t));
                }
            }
            Tok::RParen | Tok::RBracket | Tok::Comma => {
                close_in_bracket(&mut stack, &mut out, depth, &t, &virt)
            }
            Tok::RBrace => {
                // Close implicit blocks inside the explicit block, then it.
                if let Some(i) = stack.iter().rposition(|c| matches!(c, Ctx::Explicit)) {
                    for c in stack.drain(i..).rev() {
                        if let Ctx::Implicit { .. } = c {
                            out.push(virt(Tok::VRBrace, &t));
                        }
                    }
                }
            }
            Tok::Kw(k @ (Kw::Then | Kw::Else | Kw::Of)) => {
                let want = if *k == Kw::Of { Kw::Case } else { Kw::If };
                if let Some(i) = stack
                    .iter()
                    .rposition(|c| matches!(c, Ctx::Marker(m) if *m == want))
                {
                    for c in stack.drain(i + 1..).rev() {
                        if let Ctx::Implicit { .. } = c {
                            out.push(virt(Tok::VRBrace, &t));
                        }
                    }
                    if *k != Kw::Then {
                        stack.pop();
                    }
                }
            }
            _ => {}
        }
        push_tok(&mut out, &mut stack, &mut depth, &mut pending, t);
    }
    out
}

fn close_in_bracket(
    stack: &mut Vec<Ctx>,
    out: &mut Vec<Token>,
    depth: u32,
    t: &Token,
    virt: &impl Fn(Tok, &Token) -> Token,
) {
    if depth == 0 {
        return;
    }
    while let Some(c) = stack.last() {
        match c {
            Ctx::Implicit { depth: d, .. } if *d >= depth && depth > 0 => {
                stack.pop();
                out.push(virt(Tok::VRBrace, t));
            }
            Ctx::Marker(_) => {
                // An unfinished `if`/`case` inside the bracket.
                if stack
                    .iter()
                    .rev()
                    .skip(1)
                    .any(|c| matches!(c, Ctx::Implicit { depth: d, .. } if *d >= depth))
                {
                    stack.pop();
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
}

fn push_tok(
    out: &mut Vec<Token>,
    stack: &mut Vec<Ctx>,
    depth: &mut u32,
    pending: &mut Option<bool>,
    t: Token,
) {
    match &t.tok {
        Tok::Kw(Kw::Where | Kw::Do | Kw::Of) => *pending = Some(false),
        Tok::Kw(Kw::Let) => *pending = Some(true),
        Tok::Kw(k @ (Kw::If | Kw::Case)) => stack.push(Ctx::Marker(*k)),
        Tok::LParen | Tok::LBracket => *depth += 1,
        Tok::RParen | Tok::RBracket => *depth = depth.saturating_sub(1),
        _ => {}
    }
    out.push(t);
}
