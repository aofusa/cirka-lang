//! Mutable references (`MUTVAR`, 05-runtime.md §1): `IORef` / `STRef`.
//! One RC-managed word holds the current value. A reference that (through
//! its value) points to itself forms a cycle and leaks (04-memory-model.md
//! §5); that is not a safety violation.

use crate::alloc;
use crate::object::{self, Obj};

/// A fresh reference holding `v` (borrowed; the reference takes its own
/// count).
///
/// # Safety
/// `v` must be a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mutvar_new(v: Obj) -> Obj {
    // SAFETY: fresh allocation of header + one word; `v` is live.
    unsafe {
        object::cirka_dup(v);
        let o = alloc::cirka_alloc(16) as Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 1;
        (*o).kind = object::MUTVAR;
        *object::word(o, 0) = v as usize;
        o
    }
}

/// The current value (a new count for the caller).
///
/// # Safety
/// `r` must be a live MUTVAR (borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mutvar_read(r: Obj) -> Obj {
    // SAFETY: `r` is a live MUTVAR whose word 0 is a live object.
    unsafe {
        let v = *object::word(r, 0) as Obj;
        object::cirka_dup(v);
        v
    }
}

/// Replaces the value by `v` (borrowed) and releases the old one.
///
/// # Safety
/// `r` must be a live MUTVAR and `v` a live object (both borrowed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mutvar_write(r: Obj, v: Obj) -> i64 {
    // SAFETY: as above; the old value is released after the store, so a
    // finalizer that reads `r` sees the new value.
    unsafe {
        object::cirka_dup(v);
        let old = *object::word(r, 0) as Obj;
        *object::word(r, 0) = v as usize;
        object::cirka_drop(old);
    }
    0
}
