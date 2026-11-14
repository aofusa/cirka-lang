//! Mutable references (`MUTVAR`, 05-runtime.md §1): `IORef` / `STRef`.
//! One RC-managed word holds the current value. A reference that (through
//! its value) points to itself forms a cycle and leaks (04-memory-model.md
//! §5), which is not a safety violation, unless the program collects cycles
//! (`--cycles=collect`): then every reference is a root of the collector.
//!
//! A reference shared between threads (negative count) is read and written
//! by swapping its value word with [`BUSY`] and storing a value back
//! afterwards: the word is its own spin lock, so a reader takes its count
//! on the value before a writer can release it (05-runtime.md §9.6).

use crate::alloc;
use crate::object::{self, Obj, cirka_drop, cirka_dup, cirka_mark_shared, rc, word};
use crate::sync::backoff;
use core::sync::atomic::{AtomicUsize, Ordering};

/// The value word of a shared reference while a thread uses it (no object
/// lives at an odd address).
const BUSY: usize = 1;

/// Takes the value of the shared reference `r`, leaving [`BUSY`] until
/// [`put_back`].
///
/// # Safety
/// `r` must be a live shared MUTVAR.
unsafe fn take(r: Obj) -> Obj {
    // SAFETY: word 0 of a live MUTVAR, accessed atomically while shared.
    let w = unsafe { AtomicUsize::from_ptr(word(r, 0)) };
    let mut spins = 0;
    loop {
        let v = w.load(Ordering::Relaxed);
        if v != BUSY
            && w.compare_exchange_weak(v, BUSY, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
        {
            return v as Obj;
        }
        backoff(&mut spins);
    }
}

/// Ends [`take`], storing `v` as the value.
///
/// # Safety
/// The caller must have taken `r`.
unsafe fn put_back(r: Obj, v: Obj) {
    // SAFETY: as for `take`.
    unsafe { AtomicUsize::from_ptr(word(r, 0)).store(v as usize, Ordering::Release) }
}

/// A fresh reference holding `v` (borrowed; the reference takes its own
/// count).
///
/// # Safety
/// `v` must be a live object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mutvar_new(v: Obj) -> Obj {
    // SAFETY: fresh allocation of header + one word; `v` is live.
    unsafe {
        cirka_dup(v);
        let o = alloc::cirka_alloc(16) as Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 1;
        (*o).kind = object::MUTVAR;
        *word(o, 0) = v as usize;
        // A reference may come to point to itself (04-memory-model.md §5).
        crate::cycles::cirka_cycle_candidate(o);
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
        if rc(r) < 0 {
            let v = take(r);
            cirka_dup(v);
            put_back(r, v);
            return v;
        }
        let v = *word(r, 0) as Obj;
        cirka_dup(v);
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
        cirka_dup(v);
        let old = if rc(r) < 0 {
            // Other threads may read the value: it is shared.
            cirka_mark_shared(v);
            let old = take(r);
            put_back(r, v);
            old
        } else {
            let old = *word(r, 0) as Obj;
            *word(r, 0) = v as usize;
            old
        };
        cirka_drop(old);
    }
    0
}

/// Replaces the value by `new` if it is still `old` (the same object):
/// 1, or 0 when another value was stored meanwhile (`old` and `new` are
/// borrowed). `atomicModifyIORef` is a loop of read and compare-and-swap.
///
/// # Safety
/// `r` must be a live MUTVAR and `old`, `new` live objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_mutvar_cas(r: Obj, old: Obj, new: Obj) -> i64 {
    // SAFETY: as for `cirka_mutvar_write`.
    unsafe {
        cirka_dup(new);
        if rc(r) < 0 {
            cirka_mark_shared(new);
            let cur = take(r);
            if cur != old {
                put_back(r, cur);
                cirka_drop(new);
                return 0;
            }
            put_back(r, new);
        } else {
            if *word(r, 0) as Obj != old {
                cirka_drop(new);
                return 0;
            }
            *word(r, 0) = new as usize;
        }
        // The reference's count on the old value (the caller has its own).
        cirka_drop(old);
        1
    }
}
