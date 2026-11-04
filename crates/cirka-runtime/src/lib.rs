//! cirka-runtime: the shared runtime of aCHC and CirkaLang (`libcirka_rt.a`).
//!
//! A `no_std` static library depending only on libc. Generated code calls
//! it through the C ABI; the public functions are declared in
//! `include/cirka_rt.h`. See docs/design/05-runtime.md.
#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]
#![cfg_attr(test, allow(dead_code))]

pub mod alloc;
pub mod debug;
pub mod io;
pub mod object;
pub mod start;
pub mod sys;

use core::cell::UnsafeCell;

/// A global mutable cell. The runtime is single-threaded until the
/// concurrency runtime (FEAT-0045) adds per-thread state.
#[repr(transparent)]
pub struct Global<T>(UnsafeCell<T>);

// SAFETY: all accesses go through `get`, whose callers guarantee exclusive
// access (single-threaded runtime state).
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
    pub const fn new(v: T) -> Global<T> {
        Global(UnsafeCell::new(v))
    }

    /// # Safety
    /// The caller must guarantee that no other reference to the value is
    /// alive (the runtime state is only touched from one thread).
    #[allow(clippy::mut_from_ref)]
    #[inline(always)]
    pub unsafe fn get(&self) -> &mut T {
        // SAFETY: guaranteed by the caller.
        unsafe { &mut *self.0.get() }
    }
}

/// Allocates a BYTES object holding a copy of `len` bytes at `p`.
///
/// # Safety
/// `p` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_bytes_from(p: *const u8, len: usize) -> object::Obj {
    // SAFETY: fresh allocation large enough for header, length and data.
    unsafe {
        let o = alloc::cirka_alloc(16 + len.div_ceil(8) * 8) as object::Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = object::BYTES;
        *object::word(o, 0) = len;
        core::ptr::copy_nonoverlapping(p, object::word(o, 1) as *mut u8, len);
        o
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    io::fatal(b"cirka runtime: internal error", 3)
}

#[cfg(test)]
mod tests;

/// The precompiled `core` references the Rust personality routine from its
/// unwind tables even with `panic = "abort"`; the runtime never unwinds
/// through Rust frames, so a stub suffices.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() {}

/// A fresh zero-filled BYTES object of `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn cirka_bytes_new(len: i64) -> object::Obj {
    let len = len.max(0) as usize;
    // SAFETY: fresh allocation large enough for header, length and data.
    unsafe {
        let o = alloc::cirka_alloc(16 + len.div_ceil(8) * 8) as object::Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = object::BYTES;
        *object::word(o, 0) = len;
        core::ptr::write_bytes(object::word(o, 1) as *mut u8, 0, len.div_ceil(8) * 8);
        o
    }
}

/// A fresh ARRAY of `len` elements all equal to `init` (consumed; it is
/// dup'ed `len - 1` times, or dropped when `len == 0`).
///
/// # Safety
/// `init` must be a live object owned by the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cirka_array_new(len: i64, init: object::Obj) -> object::Obj {
    let len = len.max(0) as usize;
    // SAFETY: fresh allocation; init is live.
    unsafe {
        if len == 0 {
            object::cirka_drop(init);
        } else {
            for _ in 1..len {
                object::cirka_dup(init);
            }
        }
        let o = alloc::cirka_alloc(16 + 8 * len) as object::Obj;
        (*o).rc = 1;
        (*o).tag = 0;
        (*o).nptr = 0;
        (*o).kind = object::ARRAY;
        *object::word(o, 0) = len;
        for i in 0..len {
            *object::word(o, 1 + i) = init as usize;
        }
        o
    }
}
