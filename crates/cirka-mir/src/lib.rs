//! cirka-mir: the MIR (06-ir.md §2), its text format, ownership verifier,
//! reference interpreter, Perceus passes and the Core → MIR lowering.

pub mod big;
pub mod borrow;
#[allow(unsafe_code)] // The interpreter's FFI boundary (see the module comment).
pub mod ffi;
pub mod interp;
pub mod ir;
pub mod lower;
pub mod parse;
pub mod perceus;
pub mod print;
pub mod reuse;
pub mod uniq;
pub mod verify;

pub use cirka_core::{CSig, CTy};
pub use ir::*;
pub use parse::parse;
pub use print::print_module;

#[cfg(test)]
mod tests;
