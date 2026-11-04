//! Core optimization passes (FEAT-0033). Every pass is followed by Core
//! Lint; a pass that produces invalid Core is an internal error.

use crate::expr::Program;
use cirka_diagnostics::Diagnostic;

/// Runs the optimization pipeline.
pub fn optimize(p: &mut Program) -> Result<(), Diagnostic> {
    let _ = p;
    Ok(())
}
