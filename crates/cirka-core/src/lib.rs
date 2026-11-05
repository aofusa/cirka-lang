//! cirka-core: the common Core shared by aCHC and CirkaLang (06-ir.md §1):
//! data structures, text format (parser / printer), Core Lint, a building
//! DSL and the optimization passes.

pub mod build;
pub mod expr;
pub mod lint;
pub mod opt;
pub mod parse;
pub mod print;
pub mod sexp;
pub mod spec;
pub mod strict;
pub mod ty;
pub mod unpack;
pub mod ww;

pub use expr::*;
pub use parse::parse_program;
pub use print::print_program;
pub use ty::*;

#[cfg(test)]
mod tests;
