//! cirkalang-syntax: lexer, layout algorithm, AST and parser of CirkaLang
//! (docs/design/cirkalang/spec-v0.0.1.md §2–3).

pub mod ast;
pub mod derive;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod print;
pub mod testing;

use cirka_diagnostics::{Diagnostic, FileId};

/// Lexes, lays out and parses a module. Always returns a (possibly
/// partial) AST together with the syntax errors.
pub fn parse(file: FileId, src: &str) -> (ast::Module, Vec<Diagnostic>) {
    let (toks, mut errs) = lexer::lex(file, src);
    let toks = layout::layout(toks);
    let (m, perrs) = parser::parse_module(toks);
    errs.extend(perrs);
    (m, errs)
}

/// Parses a standalone expression.
pub fn parse_expr(file: FileId, src: &str) -> (Option<ast::Expr>, Vec<Diagnostic>) {
    let (toks, mut errs) = lexer::lex(file, src);
    let toks = layout::layout(toks);
    let (e, perrs) = parser::parse_expr_tokens(toks);
    errs.extend(perrs);
    (e, errs)
}

#[cfg(test)]
mod tests;
