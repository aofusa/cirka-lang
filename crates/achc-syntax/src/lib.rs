//! achc-syntax: lexer, layout algorithm, AST and parser of Haskell 2010
//! for aCHC (docs/design/01-architecture.md, 02-language-extensions.md).

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod print;

use cirka_diagnostics::{Diagnostic, FileId};

/// Lexes and parses a module. Always returns a (possibly partial) AST
/// together with the syntax errors.
pub fn parse(file: FileId, src: &str) -> (ast::Module, Vec<Diagnostic>) {
    let lexed = lexer::lex(file, src);
    let mut errs = lexed.errors;
    let mut p = parser::Parser::new(lexed.tokens);
    let m = p.module(lexed.extensions);
    errs.extend(p.finish());
    (m, errs)
}

/// Parses a standalone expression (tests and the REPL-like tools).
pub fn parse_expr(file: FileId, src: &str) -> (Option<ast::Expr>, Vec<Diagnostic>) {
    let lexed = lexer::lex(file, src);
    let mut errs = lexed.errors;
    let mut p = parser::Parser::new(lexed.tokens);
    let e = p.expr().ok();
    errs.extend(p.finish());
    (e, errs)
}

#[cfg(test)]
mod tests;
