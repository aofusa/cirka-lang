//! cirka-mir: the MIR (06-ir.md §2), its text format, ownership verifier,
//! reference interpreter, Perceus passes and the Core → MIR lowering.

pub mod ir;
pub mod parse;
pub mod print;
pub mod verify;

pub use ir::*;
pub use parse::parse;
pub use print::print_module;

#[cfg(test)]
mod tests;
