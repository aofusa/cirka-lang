//! cirkalang-elab: elaboration of resolved CirkaLang into TT, the
//! dependently typed core language, with multiplicity (QTT) checking
//! (spec-v0.0.1.md §4).

pub mod elab;
pub mod eval;
pub mod lint;
pub mod tt;

#[cfg(test)]
mod tests;

use cirka_diagnostics::Diagnostic;
use cirkalang_resolve::rast;

/// The result of elaboration: the program (when there are no errors) and
/// all diagnostics (errors and warnings).
pub struct Elaborated {
    pub program: Option<tt::Program>,
    pub diags: Vec<Diagnostic>,
}

/// Elaborates and type checks a resolved program. In debug builds the
/// result is re-checked by the TT Lint.
pub fn elaborate(rp: &rast::Program) -> Elaborated {
    let mut e = elab::Elab::new(rp);
    e.run();
    let mut diags = dedup(e.diags.take());
    if diags.iter().any(|d| d.is_error()) {
        return Elaborated {
            program: None,
            diags,
        };
    }
    let prog = e.prog;
    if cfg!(debug_assertions)
        && let Err(errs) = lint::lint_program(&prog)
    {
        for m in errs {
            diags.push(Diagnostic::ice(format!("TT Lint: {m}")));
        }
        return Elaborated {
            program: None,
            diags,
        };
    }
    Elaborated {
        program: Some(prog),
        diags,
    }
}

fn dedup(ds: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for d in ds {
        if !out.iter().any(|o| {
            o.code == d.code && o.message == d.message && o.primary_span() == d.primary_span()
        }) {
            out.push(d);
        }
    }
    out
}
